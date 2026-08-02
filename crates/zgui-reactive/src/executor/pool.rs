//! The task pool: one per UI thread, polled only by [`flush`](crate::executor::flush).

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::panic::Location;

use any_spawner::{CustomExecutor, PinnedFuture, PinnedLocalFuture};
use futures::executor::{LocalPool, LocalSpawner};
use futures::task::LocalSpawnExt;

use crate::executor::through::WakeThrough;
use crate::executor::ui_thread::is_ui_thread;

thread_local! {
    /// This thread's tasks. Borrowed for the duration of a poll, which is what makes a
    /// re-entrant poll a no-op instead of a panic.
    static POOL: RefCell<LocalPool> = RefCell::new(LocalPool::new());
    /// A handle that queues tasks without borrowing the pool, so a task may spawn a task.
    static SPAWNER: LocalSpawner = POOL.with_borrow(LocalPool::spawner);
    /// Where the next spawn came from, when it came from this crate's own API.
    static SPAWNED_AT: Cell<Option<&'static Location<'static>>> = const { Cell::new(None) };
}

/// Creates this thread's pool and its spawner.
///
/// Called while installing, because deriving the spawner borrows the pool: left until first
/// use, that borrow could fall inside a poll, where the pool is already borrowed.
pub(crate) fn prepare() {
    SPAWNER.with(|_| ());
}

/// Records the source location of the spawn that is about to happen.
///
/// The reactive engine spawns through the global executor, whose call goes through a function
/// pointer that `#[track_caller]` cannot see past. Tasks spawned through this crate's own
/// [`spawn_local`](crate::executor::spawn_local) leave their location here instead, so a task
/// that misbehaves can be named.
pub(crate) fn note_spawn_location(location: &'static Location<'static>) {
    SPAWNED_AT.set(Some(location));
}

/// The single-threaded executor the whole framework runs on.
///
/// There is no thread pool and no reactor behind it: tasks live on the UI thread and advance
/// only when the frame loop flushes them.
pub(crate) struct UiExecutor;

impl CustomExecutor for UiExecutor {
    /// Spawns a `Send` future — on the UI thread, exactly like a local one.
    ///
    /// The reactive engine reaches this path from constructors that have no local variant, so
    /// treating it as anything other than `spawn_local` would put reactive work on a second
    /// thread by accident.
    fn spawn(&self, future: PinnedFuture<()>) {
        debug_assert!(
            is_ui_thread(),
            "reactive tasks may only be spawned from the thread that installed the runtime"
        );
        push(WakeThrough::new(future, SPAWNED_AT.take()));
    }

    fn spawn_local(&self, future: PinnedLocalFuture<()>) {
        debug_assert!(
            is_ui_thread(),
            "reactive tasks may only be spawned from the thread that installed the runtime"
        );
        push(WakeThrough::new(future, SPAWNED_AT.take()));
    }

    fn poll_local(&self) {
        poll();
    }
}

/// Queues a wrapped task on this thread's pool.
fn push<F>(task: WakeThrough<F>)
where
    F: Future<Output = ()> + Unpin + 'static,
{
    SPAWNER.with(|spawner| {
        spawner
            .spawn_local(task)
            .expect("the task pool outlives the thread that owns it");
    });
}

/// Polls every ready task to a stall.
///
/// A poll that arrives while one is already running — an effect that flushes — is dropped
/// rather than nested, because the pool cannot be borrowed twice and a nested executor is a
/// panic in the layer below.
pub(crate) fn poll() {
    let polled = POOL.with(|pool| match pool.try_borrow_mut() {
        Ok(mut pool) => {
            pool.run_until_stalled();
            true
        }
        Err(_) => false,
    });
    if !polled {
        tracing::debug!("a reactive flush was requested from inside one, and was ignored");
    }
}
