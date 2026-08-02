//! Cancellation attached to the current owner.

use reactive_graph::owner::Owner;
use send_wrapper::SendWrapper;

use crate::executor::{assert_owner, assert_ui_thread};

/// Runs `cleanup` when the current owner is disposed of.
///
/// This is the "on unmount" hook: the owner of a mounted node is disposed of synchronously when
/// that node unmounts, so anything registered here — cancelling a timer, deregistering an
/// observer, removing a row from a shared table — happens before the next frame, not on some
/// later tick.
///
/// The closure need not be `Send`, which is the whole point: cleanups capture node handles,
/// element references and view state, none of which are. It is run on the thread it was created
/// on, and dropping it on any other thread panics rather than corrupting what it captured.
///
/// Cleanups run in the reverse order of registration, after every child owner has been disposed
/// of and before the owner's stored values are dropped. An effect re-running counts as a
/// disposal of its own scope, so a cleanup registered inside an effect runs before that effect's
/// next run.
///
/// ```
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use zgui_reactive::{Mounted, install, on_cleanup_local};
///
/// install().unwrap();
/// let cancelled = Rc::new(Cell::new(false));
///
/// let node = Mounted::new();
/// node.with({
///     let cancelled = Rc::clone(&cancelled);
///     move || on_cleanup_local(move || cancelled.set(true))
/// });
///
/// assert!(!cancelled.get());
/// node.unmount();
/// assert!(cancelled.get());
/// ```
///
/// # Panics
///
/// In debug builds, if there is no current owner — the closure would otherwise be dropped
/// immediately and silently, and whatever it was meant to cancel would never be cancelled.
#[track_caller]
pub fn on_cleanup_local(cleanup: impl FnOnce() + 'static) {
    assert_ui_thread("on_cleanup_local");
    assert_owner("on_cleanup_local");
    let cleanup = SendWrapper::new(cleanup);
    Owner::on_cleanup(move || cleanup.take()());
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::executor::install;

    #[test]
    fn cleanups_run_when_the_owner_is_disposed_of() {
        install().unwrap();
        let owner = Owner::new();
        let calls = Rc::new(Cell::new(0));

        owner.with({
            let calls = Rc::clone(&calls);
            move || on_cleanup_local(move || calls.set(calls.get() + 1))
        });

        assert_eq!(calls.get(), 0);
        owner.cleanup();
        assert_eq!(calls.get(), 1);
        owner.cleanup();
        assert_eq!(calls.get(), 1, "a cleanup runs once");
    }

    #[test]
    fn a_cleanup_may_capture_a_value_that_is_not_send() {
        install().unwrap();
        let owner = Owner::new();
        let seen = Rc::new(Cell::new(false));
        let captured = Rc::new(7u8);

        owner.with({
            let seen = Rc::clone(&seen);
            move || on_cleanup_local(move || seen.set(*captured == 7))
        });
        owner.cleanup();
        assert!(seen.get());
    }
}
