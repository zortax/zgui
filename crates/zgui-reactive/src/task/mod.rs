//! Tasks: what runs on the UI thread, what runs off it, and what cancels either.
//!
//! Three kinds of work, and the whole point of the module is that the first is where a caller
//! writes the other two down.
//!
//! * **On the UI thread** — [`spawn`] and [`spawn_local`]. Polled by [`flush`](crate::flush), so
//!   they may touch signals, the document and view state, and so a future that blocks here blocks
//!   the frame.
//! * **Off it** — [`background`] and [`blocking`], which return futures, so the round trip is one
//!   `await` inside a UI task and the value arrives back on the UI thread.
//! * **Onto it** — [`ui`], a `Send` handle any thread can post a closure to.
//!
//! # Cancellation
//!
//! A task is cancelled when the scope that spawned it is disposed of. That is almost always what
//! is wanted: a row that fetches its thumbnail should stop when it scrolls out of the list, and a
//! task that survived its component would write into signals whose arena entries are gone.
//!
//! Cancellation drops what the future captured, and does it *inside* the unmount rather than at
//! the next flush — the same synchronous-disposal rule [`Mounted`](crate::Mounted) is built on.
//!
//! The handle [`spawn`] returns is for cancelling *early*. Dropping it does **not** cancel the
//! task, which is the one place this module departs from the framework's usual "the handle is the
//! lifetime" shape — the view layer's `TimeoutHandle` and `IntervalHandle` do cancel on drop. A
//! timer is a standing registration whose handle is the only thing that names it; a task is a
//! piece of work, and the form nine callers in ten write is
//! `on:click = move |_| { spawn(async { … }); }`, where the handle dies at the semicolon.
//! Cancelling there would mean nothing ever ran.
//!
//! Use [`spawn_detached`] for the rare task that must outlive its scope — a save on the way out,
//! a metric being flushed.
//!
//! ```
//! use zgui_reactive::prelude::*;
//! use zgui_reactive::{Mounted, RwSignal, flush, install, spawn_local};
//!
//! install().expect("no other executor is installed");
//! let node = Mounted::new();
//!
//! let seen = node.with(|| {
//!     let seen = RwSignal::new(0);
//!     spawn_local(async move { seen.set(1) });
//!     seen
//! });
//!
//! assert_eq!(seen.get(), 0, "a task runs at the flush, not at the spawn");
//! flush();
//! assert_eq!(seen.get(), 1);
//! node.unmount();
//! ```

mod background;
mod cancel;
mod set;
mod stream;
mod threads;
mod ui;

use std::future::Future;
use std::panic::Location;
use std::rc::Rc;

pub use background::{
    Background, BackgroundSpawner, SpawnerError, background, blocking, set_background_spawner,
};
pub use set::provide_task_set;
pub use stream::{signal_from_stream, spawn_stream};
pub use ui::{Ui, ui};

pub(crate) use ui::drain as drain_ui_queue;

use crate::executor::assert_ui_thread;
use crate::task::cancel::{Cancel, Cancellable};

/// A running task, and the way to stop it before it finishes.
///
/// Dropping this does not cancel the task — the scope that spawned it does that. See the module
/// documentation for why this differs from the framework's other handles.
#[derive(Clone)]
pub struct Task(Rc<Cancel>);

impl Task {
    /// Stops the task now, dropping whatever its future captured.
    ///
    /// Cancelling a task that has already finished does nothing.
    pub fn cancel(&self) {
        self.0.cancel();
    }

    /// Whether the task has finished or been cancelled.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.0.is_over()
    }
}

impl core::fmt::Debug for Task {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Task")
            .field("finished", &self.is_finished())
            .finish()
    }
}

/// Runs `future` on the UI thread, until it finishes or its scope goes away.
///
/// The variant to use for anything that touches the document, a node handle or a view: those types
/// are deliberately not `Send`, and the executor never moves a task between threads.
///
/// The future is polled by [`flush`](crate::flush), never before and never elsewhere. A wake from
/// any thread — a background task finishing, a channel receiving, a timer firing — asks the
/// platform for the frame that will poll it.
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn spawn_local(future: impl Future<Output = ()> + 'static) -> Task {
    assert_ui_thread("spawn_local");
    let task = Cancel::new(future);
    match set::current_or_install() {
        Some(set) => set.insert(Rc::clone(&task)),
        None => tracing::debug!(
            "a task was spawned with no owner current, so nothing will cancel it; \
             spawn inside a component, or say so with `spawn_detached`"
        ),
    }
    push(Rc::clone(&task), Location::caller());
    Task(task)
}

/// Runs `future` on the UI thread, until it finishes or its scope goes away.
///
/// Identical to [`spawn_local`] in every respect but the bound. `Send` is asked for only so that
/// this can stand in for a thread-pool spawn in code that cannot know it is single-threaded; the
/// future runs on the UI thread either way. To actually get off the UI thread, `await`
/// [`background`] from inside one of these.
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Task {
    spawn_local(future)
}

/// Runs `future` on the UI thread until it finishes, whatever happens to the scope that spawned it.
///
/// For work that must complete because something outside the program depends on it: a save, an
/// acknowledgement, a metric. Everything else wants [`spawn`], because a detached task that writes
/// a signal belonging to a disposed scope writes into nothing, silently, and does it a frame after
/// the component it belonged to stopped existing.
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn spawn_detached(future: impl Future<Output = ()> + 'static) {
    assert_ui_thread("spawn_detached");
    push(Cancel::new(future), Location::caller());
}

/// Hands a wrapped task to this thread's pool.
fn push(task: Rc<Cancel>, at: &'static Location<'static>) {
    crate::executor::pool::note_spawn_location(at);
    any_spawner::Executor::spawn_local(Box::pin(Cancellable::new(task)));
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc as StdRc;
    use std::sync::Arc;

    use reactive_graph::owner::Owner;

    use super::*;
    use crate::executor::{TestWaker, flush, install, set_frame_waker};
    use crate::own::Mounted;

    /// Installs a runtime and a counting waker on this test's thread.
    fn runtime() -> Arc<TestWaker> {
        install().expect("no other executor is installed");
        let waker = Arc::new(TestWaker::default());
        set_frame_waker(waker.clone());
        waker
    }

    /// A value that records when it is dropped.
    struct Witness(StdRc<Cell<bool>>);

    impl Drop for Witness {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    #[test]
    fn unmounting_cancels_a_task_and_drops_its_captures_at_once() {
        let _waker = runtime();
        let dropped = StdRc::new(Cell::new(false));
        let node = Mounted::new();

        let task = node.with(|| {
            let witness = Witness(StdRc::clone(&dropped));
            spawn_local(async move {
                let _held = witness;
                std::future::pending::<()>().await;
            })
        });

        flush();
        assert!(!task.is_finished());
        assert!(!dropped.get());

        node.unmount();
        assert!(task.is_finished(), "the unmount cancelled it");
        assert!(
            dropped.get(),
            "and released what it captured inside `unmount`, not a frame later"
        );
    }

    #[test]
    fn a_cancelled_task_never_runs_again() {
        let _waker = runtime();
        let runs = StdRc::new(Cell::new(0));
        let owner = Owner::new();

        let task = owner.with(|| {
            let runs = StdRc::clone(&runs);
            spawn_local(async move {
                runs.set(runs.get() + 1);
                std::future::pending::<()>().await;
            })
        });

        flush();
        assert_eq!(runs.get(), 1);

        task.cancel();
        flush();
        flush();
        assert_eq!(runs.get(), 1, "cancelling stopped it");
        owner.cleanup();
    }

    #[test]
    fn a_detached_task_survives_its_scope() {
        let _waker = runtime();
        let ran = StdRc::new(Cell::new(false));
        let node = Mounted::new();

        node.with(|| {
            let ran = StdRc::clone(&ran);
            spawn_detached(async move { ran.set(true) });
        });

        node.unmount();
        flush();
        assert!(ran.get(), "a detached task is not the scope's to cancel");
    }

    #[test]
    fn many_spawns_under_one_owner_leave_nothing_behind() {
        let _waker = runtime();
        let node = Mounted::new();

        // What a button clicked ten thousand times does.
        node.with(|| {
            for _ in 0..10_000 {
                spawn_local(async {});
            }
        });
        flush();

        let live = node.with(|| {
            set::current_or_install()
                .expect("an owner is current")
                .len()
        });
        assert_eq!(live, 0, "the owner is still holding {live} finished tasks");
        node.unmount();
    }

    #[test]
    fn a_task_woken_from_another_thread_asks_for_exactly_one_frame() {
        let waker = runtime();
        let node = Mounted::new();
        let done = StdRc::new(Cell::new(false));

        node.with(|| {
            let done = StdRc::clone(&done);
            spawn_local(async move {
                let value = background(async { 6 * 7 }).await;
                done.set(value == 42);
            });
        });

        // The first flush starts the background work and parks on the oneshot.
        flush();
        waker.take();

        // Wait for the worker without spinning the reactive runtime.
        let mut frames = 0;
        while waker.count() == 0 && frames < 1_000 {
            std::thread::yield_now();
            frames += 1;
        }
        assert_eq!(waker.take(), 1, "the finished worker asked for one frame");

        flush();
        assert!(done.get(), "and that frame delivered the value");
        node.unmount();
    }
}
