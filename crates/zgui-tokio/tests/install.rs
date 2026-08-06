//! What installing a runtime buys, asserted against a real one.
//!
//! Every test drives the frame loop by hand: `flush` is what polls the UI thread's tasks, so a
//! test with no window plays the part of one by calling it. `TestWaker` stands in for the
//! platform's redraw request, which is how "the work became ready" is observable without a
//! compositor.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use zgui_reactive::prelude::*;
use zgui_reactive::{Mounted, RwSignal, TestWaker, flush, install, set_frame_waker, spawn_local};

/// The one runtime this test binary installs.
///
/// zgui's background executor is one per process by design, and cargo runs every test in this file
/// in one process — so the tests share an installation exactly as two windows of one application
/// would, rather than each trying to claim the slot.
static TOKIO: OnceLock<zgui_tokio::Installed> = OnceLock::new();

/// Installs the reactive runtime, a counting waker, and this thread's entry into the runtime.
fn runtime() -> Arc<TestWaker> {
    install().expect("no other executor is installed");
    let waker = Arc::new(TestWaker::default());
    set_frame_waker(waker.clone());

    let tokio = TOKIO.get_or_init(|| zgui_tokio::install().expect("a runtime"));
    // Per thread, because entering the runtime is a property of one thread's flush while the
    // background executor is a property of the process.
    zgui_tokio::enter_here(tokio.handle());
    waker
}

/// Flushes until `settled` answers true, or gives up so a failure is a failure and not a hang.
fn pump(waker: &TestWaker, settled: impl Fn() -> bool) {
    for _ in 0..2_000 {
        flush();
        if settled() {
            return;
        }
        // The reactive layer never blocks the UI thread, so a test standing in for the event loop
        // is the thing that has to wait for the runtime's threads to make progress.
        std::thread::sleep(Duration::from_millis(1));
        let _ = waker.take();
    }
    panic!("the work never became ready");
}

#[test]
fn a_runtime_alongside_the_reactive_one_does_not_take_its_executor_slot() {
    // The order a real application uses: zgui claims `any_spawner`'s single process-wide slot with
    // its own UI-thread executor, and tokio must not be competing for it. The failure this rules
    // out surfaces as `InstallError::ForeignExecutor` from somewhere that never mentions tokio.
    let _waker = runtime();
    install().expect("the reactive runtime is still installable with tokio in place");
}

#[test]
fn a_ui_thread_task_may_await_a_tokio_timer() {
    let waker = runtime();
    let node = Mounted::new();

    // The point of entering the runtime on every flush: `sleep` is *constructed* inside the poll
    // of the task that awaits it, and without a runtime context that construction panics.
    let elapsed = node.with(|| {
        let elapsed = RwSignal::new(false);
        spawn_local(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            elapsed.set(true);
        });
        elapsed
    });

    pump(&waker, || elapsed.get_untracked());
    assert!(elapsed.get_untracked());
    node.unmount();
}

#[test]
fn background_work_runs_on_the_runtime_and_comes_back_to_the_ui_thread() {
    let waker = runtime();
    let node = Mounted::new();

    let answer = node.with(|| {
        let answer = RwSignal::new(0_u32);
        spawn_local(async move {
            let computed = zgui_reactive::background(async {
                // Runs on a tokio worker, which is a runtime context, so this is legal here and
                // would not be on the default executor.
                tokio::time::sleep(Duration::from_millis(10)).await;
                6 * 7
            })
            .await;
            answer.set(computed);
        });
        answer
    });

    pump(&waker, || answer.get_untracked() != 0);
    assert_eq!(answer.get_untracked(), 42);
    node.unmount();
}

#[test]
fn a_watch_channel_becomes_a_signal_and_stops_with_its_scope() {
    let waker = runtime();
    let (send, receive) = tokio::sync::watch::channel(1_u8);
    let node = Mounted::new();

    let latest = node.with(|| zgui_tokio::watch_signal(receive));
    assert_eq!(
        latest.get_untracked(),
        1,
        "it starts at what the channel holds"
    );

    send.send(2).expect("the receiver is alive");
    pump(&waker, || latest.get_untracked() == 2);

    // The subscription belongs to the scope, so unmounting ends it rather than leaving a task
    // writing into a signal whose arena entry has gone.
    node.unmount();
    assert!(
        send.send(3).is_err(),
        "the unmount dropped the receiver the task was holding"
    );
}

#[test]
fn an_mpsc_receiver_delivers_every_message_on_the_ui_thread() {
    let waker = runtime();
    let (send, receive) = tokio::sync::mpsc::channel(8);
    let node = Mounted::new();

    let seen = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&seen);
    node.with(|| {
        zgui_tokio::spawn_receiver(receive, move |_item: u8| {
            counter.fetch_add(1, Ordering::SeqCst);
        })
    });

    let sender = send.clone();
    std::thread::spawn(move || {
        for item in 0..5 {
            sender.blocking_send(item).expect("the receiver is alive");
        }
    })
    .join()
    .unwrap();
    drop(send);

    pump(&waker, || seen.load(Ordering::SeqCst) == 5);
    assert_eq!(seen.load(Ordering::SeqCst), 5);
    node.unmount();
}
