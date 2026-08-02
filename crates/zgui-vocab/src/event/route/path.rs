//! What a path of elements has to be able to say about its registrations.

use crate::event::kind::EventKind;
use crate::event::listener::ListenerOptions;

/// A path from a tree's root down to the element an event was aimed at, and what listens on it.
///
/// Implement this over whatever holds the registrations. It is asked only for how the listeners
/// on one element were registered, one element at a time, so an implementation never has to
/// gather them into a collection of its own — which is what keeps resolving an event free of
/// allocation on the tree side as well as on the answer's.
///
/// The visitor takes a position rather than a name for the same reason
/// [`RouteStep`](crate::RouteStep) reports one: the vocabulary has no opinion about what a
/// registration is called, and every caller already has its own name for it.
pub trait Listeners {
    /// How many elements are on the path. The root is `0` and the target is `depth() - 1`.
    ///
    /// A depth of zero is a path to nothing, which resolves to no steps at all.
    fn depth(&self) -> usize;

    /// Calls `each` once per registration for `kind` on the element at `element`, in the order
    /// they were made, with that registration's position and how it was registered.
    ///
    /// Registrations for other events are not offered: an element that listens for three events
    /// contributes positions for the one being resolved, numbered among the registrations for
    /// that event alone.
    fn each(&self, element: usize, kind: EventKind, each: &mut dyn FnMut(usize, ListenerOptions));
}

/// A path spelled out as a slice per element, for a caller that already has one.
///
/// Every element's registrations are taken to be for whichever event is being resolved, so this
/// is the shape a test reaches for and not the shape a document uses.
///
/// ```
/// use zgui_vocab::{EventKind, ListenerOptions, Listeners, Path};
///
/// let elements: [&[ListenerOptions]; 2] = [&[ListenerOptions::CAPTURE], &[]];
/// let path = Path::new(&elements);
/// assert_eq!(path.depth(), 2);
///
/// let mut seen = 0;
/// path.each(0, EventKind::Click, &mut |_, _| seen += 1);
/// assert_eq!(seen, 1);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Path<'a> {
    /// One slice of registrations per element, root first.
    elements: &'a [&'a [ListenerOptions]],
}

impl<'a> Path<'a> {
    /// A path over `elements`, root first and target last.
    pub const fn new(elements: &'a [&'a [ListenerOptions]]) -> Self {
        Self { elements }
    }
}

impl Listeners for Path<'_> {
    fn depth(&self) -> usize {
        self.elements.len()
    }

    fn each(&self, element: usize, _kind: EventKind, each: &mut dyn FnMut(usize, ListenerOptions)) {
        let Some(registrations) = self.elements.get(element) else {
            return;
        };
        for (position, options) in registrations.iter().enumerate() {
            each(position, *options);
        }
    }
}
