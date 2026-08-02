//! Focus arriving at or leaving an element.

use crate::a11y::NodeId;

/// Why focus moved, which decides whether a focus ring should be drawn.
///
/// A ring drawn on every focus change is noise for a user driving with a mouse, and its absence is
/// a barrier for a user driving with a keyboard. The distinction is made once, here, by whatever
/// moved the focus, rather than guessed at by each control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FocusCause {
    /// A pointer pressed the element.
    Pointer,
    /// The keyboard moved focus, by tabbing or by an arrow within a composite control.
    #[default]
    Keyboard,
    /// The program moved focus, without the user asking.
    Programmatic,
    /// The window itself gained or lost focus, and this element was the one holding it.
    Window,
}

impl FocusCause {
    /// Whether focus arriving this way should show a focus ring.
    pub const fn shows_ring(self) -> bool {
        matches!(self, Self::Keyboard | Self::Programmatic)
    }
}

/// What a focus event carries.
///
/// The other element is the one focus came from, on an arrival, and the one it went to, on a
/// departure. It is absent when focus came from or went to nothing at all — the window being
/// activated, or the document being clicked on empty space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusEvent {
    /// The element on the other end of the move.
    pub related: Option<NodeId>,
    /// Why focus moved.
    pub cause: FocusCause,
}

impl FocusEvent {
    /// A focus event with the given cause and no other element involved.
    pub const fn new(cause: FocusCause) -> Self {
        Self {
            related: None,
            cause,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FocusCause, FocusEvent};
    use crate::a11y::NodeId;

    #[test]
    fn only_a_pointer_suppresses_the_focus_ring() {
        assert!(!FocusCause::Pointer.shows_ring());
        assert!(FocusCause::Keyboard.shows_ring());
        assert!(FocusCause::Programmatic.shows_ring());
    }

    #[test]
    fn the_related_element_is_optional() {
        assert_eq!(FocusEvent::new(FocusCause::Window).related, None);
        let event = FocusEvent {
            related: Some(NodeId(3)),
            cause: FocusCause::Keyboard,
        };
        assert_eq!(event.related, Some(NodeId(3)));
    }
}
