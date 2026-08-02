//! The moment a dismissal that has not finished is finished anyway.

use core::time::Duration;

use zgui::prelude::set_timeout;
use zgui::view::TimeoutHandle;

/// How long content is given to play its exit before it is taken away regardless.
///
/// Everything about unmounting waits on something asynchronous: an animation end, a transition
/// end, or a cancellation of either. That is the right way round — a duration written in Rust and
/// a duration written in CSS drift the first time anyone edits the sheet — but it makes the
/// content's departure depend on an event, and an event can fail to arrive. A style whose rule
/// stopped matching mid-flight, a node the cascade replaced under a running animation, an end
/// delivered to an element that is no longer the one being watched: each leaves content mounted
/// with nothing left to unmount it.
///
/// What that costs is out of all proportion to the fault. A modal surface that stays mounted keeps
/// its scrim over the whole window and its focus trap around a subtree nobody can see, so the
/// window answers no press and no key for the rest of the session — from one dropped event.
///
/// So a dismissal that has been asked for completes, late, rather than never. A second is far
/// longer than any exit this library ships — the slowest is a fifth of that — and short enough
/// that a person who pressed Escape and saw nothing happen is still watching when it does.
pub(crate) const EXIT_DEADLINE: Duration = Duration::from_secs(1);

/// Runs `finish` one [`EXIT_DEADLINE`] from now, unless the handle is dropped first.
pub(crate) fn arm(finish: impl FnOnce() + 'static) -> TimeoutHandle {
    set_timeout(EXIT_DEADLINE, finish)
}
