//! One entry of a resolved order.

use crate::event::listener::Phase;

/// One listener to run, named by where it sits rather than by what it is.
///
/// Both numbers are positions into what the caller supplied: `element` indexes the path from its
/// root, and `registration` indexes that element's registrations for this event in the order they
/// were made. Nothing here holds a handler, an element or a tree — resolving the order and
/// running the handlers are separate jobs, and separating them is what lets the layer that owns
/// the tree resolve without being able to call into the layer that owns the handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RouteStep {
    /// Which element of the path, counting from the root.
    pub element: usize,
    /// Which of that element's registrations for this event, counting from the first made.
    pub registration: usize,
    /// Which leg of the delivery this step belongs to.
    pub phase: Phase,
}
