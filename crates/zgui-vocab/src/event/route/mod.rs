//! The order listeners run in, for one event on one path.
//!
//! An event is delivered along the path from the root of the tree down to the element it was
//! aimed at, and then back up again. Which registrations run on the way down, which run at the
//! bottom, and which run on the way up is a rule — not a property of any particular tree — and
//! this module is the one place it is written.
//!
//! It is here, in the vocabulary, for the same reason [`EventKind`] and
//! [`ListenerOptions`](crate::ListenerOptions) are:
//! two unrelated implementations of a node tree must deliver an event in the same order, or a
//! component that works against one is subtly wrong against the other. Nothing in this module
//! knows what a node is. A path is a run of positions, a registration is a position within one of
//! them, and the answer is a list of positions — so a caller keeps its own names for both and
//! this rule is stated once.
//!
//! ```
//! use zgui_vocab::{EventKind, ListenerOptions, Path, Phase, route};
//!
//! // A button inside a toolbar inside the root, with the toolbar watching on the way down and
//! // the button on the way up.
//! let elements: [&[ListenerOptions]; 3] = [
//!     &[],
//!     &[ListenerOptions::CAPTURE],
//!     &[ListenerOptions::DEFAULT],
//! ];
//! let path = Path::new(&elements);
//!
//! let mut steps = Vec::new();
//! route(EventKind::Click, &path, &mut steps);
//!
//! assert_eq!(steps.len(), 2);
//! assert_eq!((steps[0].element, steps[0].phase), (1, Phase::Capture));
//! assert_eq!((steps[1].element, steps[1].phase), (2, Phase::Target));
//! ```

mod path;
mod step;

use crate::event::kind::EventKind;
use crate::event::listener::Phase;

pub use crate::event::route::path::{Listeners, Path};
pub use crate::event::route::step::RouteStep;

/// Resolves which registrations on `path` run for `kind`, in the order they run in.
///
/// `out` is cleared first and then filled, so a caller that dispatches many events keeps one
/// buffer and this allocates nothing after the first event.
///
/// The rule, in full:
///
/// * **on the way down**, every element from the root to the one before the target, in that
///   order, contributes the registrations that asked to run on the way down;
/// * **at the target**, every registration on it contributes, however it was registered, because
///   at the element the event was aimed at there is no up or down to tell apart;
/// * **on the way up**, every element from the one before the target back to the root contributes
///   the registrations that did not ask to run on the way down — and this leg is skipped entirely
///   for an event that does not travel upwards, such as a scroll.
///
/// Within one element and one leg, registrations keep the order they were made in.
///
/// The way down happens for every event, including one that does not bubble. That asymmetry is
/// deliberate and it is what a dismissable overlay depends on: it hears about a press anywhere
/// beneath it without the pressed element cooperating, and it must hear about it *first*.
///
/// ```
/// use zgui_vocab::{EventKind, ListenerOptions, Path, Phase, route};
///
/// // A scroll does not bubble, so an ancestor listening on the way up never sees it — while one
/// // listening on the way down does.
/// let elements: [&[ListenerOptions]; 2] =
///     [&[ListenerOptions::DEFAULT, ListenerOptions::CAPTURE], &[]];
/// let path = Path::new(&elements);
///
/// let mut steps = Vec::new();
/// route(EventKind::Scroll, &path, &mut steps);
///
/// assert_eq!(steps.len(), 1);
/// assert_eq!(steps[0].registration, 1);
/// assert_eq!(steps[0].phase, Phase::Capture);
/// ```
pub fn route(kind: EventKind, path: &dyn Listeners, out: &mut Vec<RouteStep>) {
    out.clear();
    let depth = path.depth();
    let Some(target) = depth.checked_sub(1) else {
        return;
    };

    for element in 0..target {
        path.each(element, kind, &mut |registration, options| {
            if options.runs_in(Phase::Capture) {
                out.push(RouteStep {
                    element,
                    registration,
                    phase: Phase::Capture,
                });
            }
        });
    }

    path.each(target, kind, &mut |registration, _| {
        out.push(RouteStep {
            element: target,
            registration,
            phase: Phase::Target,
        });
    });

    if !kind.bubbles() {
        return;
    }
    for element in (0..target).rev() {
        path.each(element, kind, &mut |registration, options| {
            if options.runs_in(Phase::Bubble) {
                out.push(RouteStep {
                    element,
                    registration,
                    phase: Phase::Bubble,
                });
            }
        });
    }
}

#[cfg(test)]
mod tests;
