//! The end of a UI thread's reactive work.
//!
//! The task pool lives in a thread-local. Tasks still in it when the thread ends are dropped by the
//! thread's destructors, after the reactive runtime and other thread-locals are gone, and a
//! cleanup that runs there reads disposed values. [`shutdown`] empties the pool while everything is
//! still alive.

use crate::executor::ui_thread::is_ui_thread;
use crate::executor::{context, pool};

/// How many times a shutdown polls and empties the pool at most.
///
/// A drop can spawn a task, and that task is dropped on the next round. A cycle of tasks that
/// spawn on drop stops here.
const ROUNDS: u32 = 8;

/// Finishes or drops every task on this thread's pool.
///
/// Polls the pool once, so a task whose effect is gone runs to its end and drops what it holds.
/// Then drops every task that is left. Call it on the UI thread after the last window closed and
/// before the thread ends. A call off the UI thread, or from inside a flush, does nothing.
pub fn shutdown() {
    if !is_ui_thread() {
        return;
    }
    for _ in 0..ROUNDS {
        context::enter(pool::poll);
        if pool::clear() {
            return;
        }
    }
    tracing::debug!("tasks still spawned tasks after {ROUNDS} rounds of shutdown");
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use reactive_graph::effect::RenderEffect;
    use reactive_graph::signal::RwSignal;
    use reactive_graph::traits::GetUntracked;

    use super::*;
    use crate::executor::install;
    use crate::own::{Mounted, on_cleanup_local};
    use crate::task::spawn_local;

    /// Counts its drop.
    struct Witness(Rc<Cell<u32>>);

    impl Drop for Witness {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn a_task_left_pending_is_dropped_by_the_shutdown() {
        install().unwrap();
        let dropped = Rc::new(Cell::new(0));
        let witness = Witness(Rc::clone(&dropped));
        drop(spawn_local(async move {
            let _witness = witness;
            std::future::pending::<()>().await;
        }));
        crate::executor::flush();
        assert_eq!(dropped.get(), 0, "the task is pending");

        shutdown();
        assert_eq!(dropped.get(), 1, "the shutdown dropped it");
    }

    #[test]
    fn a_cleanup_a_finished_effect_holds_runs_while_the_application_scope_lives() {
        install().unwrap();
        let app = Mounted::new();
        let shared = app.with(|| RwSignal::new(7));
        let seen = Rc::new(Cell::new(None));
        // An effect whose value holds a scope of its own, as a reactive hole holds its content.
        let held = app.with(|| {
            let seen = Rc::clone(&seen);
            RenderEffect::new(move |previous: Option<Rc<Mounted>>| {
                previous.unwrap_or_else(|| {
                    let scope = Mounted::new();
                    let seen = Rc::clone(&seen);
                    scope.with(|| {
                        on_cleanup_local(move || seen.set(shared.try_get_untracked()));
                    });
                    Rc::new(scope)
                })
            })
        });
        drop(held);
        assert_eq!(
            seen.get(),
            None,
            "the effect's value is still held by its task"
        );

        shutdown();
        assert_eq!(
            seen.get(),
            Some(7),
            "the shutdown ended the task while the application's signal was alive"
        );
        app.unmount();
    }
}
