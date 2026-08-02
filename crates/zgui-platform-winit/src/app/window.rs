//! What one window remembers between events.

use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::Modifiers;

use crate::app::drag::Drag;

/// The state one window carries from one event to the next.
///
/// All three exist because the platform reports a *change* where the contract carries a *state*,
/// and the difference has to be closed on the loop's own thread where the previous value is known.
#[derive(Debug, Default)]
pub(crate) struct WindowState {
    /// Which modifiers are held.
    ///
    /// The platform reports this only when it changes, and a modifier can change while the window
    /// is not focused. A set recovered from key events alone is therefore wrong until the next
    /// press, which is how a shortcut stops working after switching windows.
    pub(crate) modifiers: Modifiers,
    /// Where the pointer was last reported.
    ///
    /// A wheel turn and a file drop carry no position of their own on any desktop protocol in use,
    /// and both have to be routed to whatever is under the pointer. This is where that comes from.
    pub(crate) pointer: Point<CssPx, Css>,
    /// Content being dragged over the window from outside.
    pub(crate) drag: Drag,
}

#[cfg(test)]
mod tests {
    use super::WindowState;
    use zgui_geom::{CssPx, Point};
    use zgui_vocab::Modifiers;

    #[test]
    fn a_window_starts_with_nothing_held_and_nothing_being_dragged() {
        let state = WindowState::default();
        assert_eq!(state.modifiers, Modifiers::NONE);
        assert_eq!(state.pointer, Point::new(CssPx(0.0), CssPx(0.0)));
        assert!(!state.drag.is_pending());
    }
}
