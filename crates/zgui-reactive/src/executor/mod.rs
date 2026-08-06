//! The executor: one task pool per UI thread, polled once per frame.
//!
//! Every effect, every async value and every timer runs as a task on the thread that called
//! [`install`], and those tasks advance only inside [`flush`]. That is what makes reactive work a
//! *phase of the frame* rather than something that happens concurrently with it, and it is why an
//! effect can touch the document at all.
//!
//! Three properties follow, and each has a guard:
//!
//! * work that becomes ready between frames still gets a frame — see [`set_frame_waker`];
//! * a cycle of effects cannot hold the frame — see [`FlushOutcome::budget_exhausted`];
//! * nothing runs on the wrong thread — see [`assert_ui_thread`].
//!
//! Nothing *reactive* ever runs on another thread. Work that is not reactive — a parse, a request,
//! a decode — does, through [`background`](crate::background), whose result comes back here; see
//! [`task`](crate::task) for the whole picture.

mod assert;
mod budget;
mod context;
pub(crate) mod frame;
pub(crate) mod pool;
mod through;
mod ui_thread;
pub(crate) mod wake;

use thiserror::Error;

pub use assert::{assert_owner, assert_ui_thread};
pub use context::{PollContext, set_poll_context};
pub use frame::{FlushOutcome, flush};
pub use ui_thread::is_ui_thread;
pub use wake::{FrameWaker, TestWaker, set_frame_waker};

pub(crate) use assert::{note_owner_children, note_owner_depth};

/// Why a reactive runtime could not be installed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InstallError {
    /// Some other executor was installed in this process first.
    ///
    /// The reactive engine allows one global executor per process. A test that starts an async
    /// runtime before the framework, or a host application that installs its own, takes the
    /// slot and reactive tasks would then run on its threads.
    #[error("another async executor is already installed in this process")]
    ForeignExecutor,
}

/// Whether this process's executor slot is held by a runtime this crate installed.
///
/// A lock rather than an atomic because the question and the claim have to be one step: two
/// threads installing at once would otherwise have the loser read the flag before the winner
/// set it, and conclude that some foreign executor had taken the slot.
static INSTALLED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

thread_local! {
    /// Whether this thread has already installed.
    static INSTALLED_HERE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Claims the calling thread as the UI thread and installs its task pool.
///
/// Call once, before creating any signal, from the thread that owns the window. Calling it
/// again on the same thread does nothing and succeeds. Each thread that installs gets its own
/// pool, its own current owner and its own frame waker; a test may therefore run a complete
/// reactive runtime per test.
///
/// After this returns, [`is_ui_thread`] is true here, [`flush`] works, and spawning is
/// permitted. Install a [`FrameWaker`] too, or work that becomes ready between frames will
/// never be asked for a frame.
///
/// # Errors
///
/// [`InstallError::ForeignExecutor`] if a different async executor already holds this process's
/// executor slot.
pub fn install() -> Result<(), InstallError> {
    if INSTALLED_HERE.get() {
        return Ok(());
    }
    let mut ours = INSTALLED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match any_spawner::Executor::init_local_custom_executor(pool::UiExecutor) {
        Ok(()) => *ours = true,
        // A later thread's pool is registered even though the process-wide slot is already
        // taken, so this only fails if something else took it.
        Err(_) if !*ours => return Err(InstallError::ForeignExecutor),
        Err(_) => {}
    }
    drop(ours);

    // Only now, so a thread whose install failed does not answer `is_ui_thread` with `true` and
    // is not left holding a pool nothing will ever poll.
    ui_thread::claim();
    pool::prepare();
    INSTALLED_HERE.set(true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use reactive_graph::effect::RenderEffect;
    use reactive_graph::owner::Owner;
    use reactive_graph::signal::RwSignal;
    use reactive_graph::traits::{Get, Set};

    use super::*;
    use crate::task::spawn_local;

    /// Installs a runtime and a counting waker on this test's thread.
    fn runtime() -> (Owner, Arc<TestWaker>) {
        install().expect("no other executor is installed");
        let waker = Arc::new(TestWaker::default());
        set_frame_waker(waker.clone());
        let owner = Owner::new();
        owner.set();
        (owner, waker)
    }

    #[test]
    fn installing_twice_on_one_thread_succeeds() {
        install().unwrap();
        install().unwrap();
        assert!(is_ui_thread());
    }

    #[test]
    fn a_spawned_task_runs_at_the_flush_and_not_before() {
        let (owner, _waker) = runtime();
        let ran = Arc::new(AtomicUsize::new(0));

        spawn_local({
            let ran = Arc::clone(&ran);
            async move {
                ran.fetch_add(1, Ordering::SeqCst);
            }
        });
        assert_eq!(ran.load(Ordering::SeqCst), 0);

        flush();
        assert_eq!(ran.load(Ordering::SeqCst), 1);
        owner.cleanup();
    }

    #[test]
    fn a_signal_write_from_another_thread_asks_for_a_frame() {
        let (owner, waker) = runtime();
        let source = RwSignal::new(0);
        let runs = Arc::new(AtomicUsize::new(0));
        let effect = RenderEffect::new({
            let runs = Arc::clone(&runs);
            move |_| {
                source.get();
                runs.fetch_add(1, Ordering::SeqCst);
            }
        });

        flush();
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "the first run is synchronous"
        );
        assert_eq!(waker.take(), 0, "a settled graph asks for nothing");

        // No input event, no frame in progress: exactly the case where the pool's own waker
        // unparks a thread that is not parked, and nothing would ever poll the task again.
        std::thread::spawn(move || source.set(1)).join().unwrap();

        assert_eq!(
            waker.take(),
            1,
            "the wake edge asked the platform for a frame"
        );
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "and ran nothing off the UI thread"
        );

        flush();
        assert_eq!(
            runs.load(Ordering::SeqCst),
            2,
            "the frame it asked for re-ran the effect"
        );

        drop(effect);
        owner.cleanup();
    }

    #[test]
    fn a_write_during_the_flush_is_folded_into_one_more_frame() {
        let (owner, waker) = runtime();
        let source = RwSignal::new(0);
        let middle = RwSignal::new(0);
        let sink = RwSignal::new(0);
        // The second effect is woken by the first one's write, from inside the flush.
        let first = RenderEffect::new(move |_| middle.set(source.get()));
        let second = RenderEffect::new(move |_| sink.set(middle.get()));
        flush();
        waker.take();

        source.set(7);
        assert_eq!(waker.take(), 1, "the write outside the frame asked for one");

        let outcome = flush();

        let _zone = crate::zone::enter_non_reactive_zone();
        assert_eq!(sink.get(), 7, "the chain settled inside one flush");
        assert!(
            outcome.needs_another_frame,
            "a wake was raised during the frame"
        );
        assert_eq!(waker.take(), 0, "and asked for no redraw from inside it");

        drop(first);
        drop(second);
        owner.cleanup();
    }

    /// A task that panics must not take the executor with it.
    ///
    /// The flush marks itself as running so a nested call cannot corrupt the pool; if the mark
    /// survived a panicking task, every later flush would look nested, do nothing and report
    /// nothing — a window that stops updating with no second error to find it by.
    #[test]
    fn a_panicking_task_leaves_the_flush_usable() {
        let (owner, _waker) = runtime();
        spawn_local(async { panic!("a task blew up") });

        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| ()));
        let flushed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(flush));
        std::panic::set_hook(previous);
        assert!(flushed.is_err(), "the panic reached the caller");

        let ran = Arc::new(AtomicUsize::new(0));
        spawn_local({
            let ran = Arc::clone(&ran);
            async move {
                ran.fetch_add(1, Ordering::SeqCst);
            }
        });

        flush();
        assert_eq!(
            ran.load(Ordering::SeqCst),
            1,
            "the next flush still polls tasks"
        );
        owner.cleanup();
    }

    /// Two effects, each writing a source the other reads, flushed under a hard deadline.
    ///
    /// Every write makes the other effect ready again, so "poll until nothing is ready" never
    /// stops. The flush runs on a thread of its own — each thread carries a complete runtime —
    /// so a failure is a test that fails in a second rather than a suite that hangs.
    #[test]
    fn two_effects_writing_each_others_sources_still_return() {
        let (report, flushed) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (owner, _waker) = runtime();
            let left = RwSignal::new(0);
            let right = RwSignal::new(0);
            let first = RenderEffect::new(move |_| right.set(left.get() + 1));
            let second = RenderEffect::new(move |_| left.set(right.get() + 1));

            let _ = report.send(flush());

            drop(first);
            drop(second);
            owner.cleanup();
        });

        let outcome = flushed
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("the flush returned instead of spinning on the cycle");
        assert!(outcome.budget_exhausted, "the cycle was cut by the budget");
        assert!(
            outcome.needs_another_frame,
            "and the work it set aside is owed a frame"
        );
    }
}
