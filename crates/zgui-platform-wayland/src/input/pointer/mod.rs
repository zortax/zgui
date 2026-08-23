//! A mouse, a finger and a stylus, as one stream told apart by a field.

pub mod axis;
pub mod button;
pub mod cursor;

use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::{PointerButton, PointerEvent, PointerId, PointerKind};

/// Where a pointer is, in the space a layout is written in.
///
/// The compositor reports a position on the surface in its own logical units, which are this
/// surface's logical pixels rather than the buffer's. That is already the space a stylesheet is
/// written in, so nothing is scaled — and nothing must be, because scaling it by the surface's own
/// factor would place every click at a fraction of where it happened on a fractional display.
pub const fn position(x: f64, y: f64) -> Point<CssPx, Css> {
    Point::new(CssPx(x as f32), CssPx(y as f32))
}

/// A mouse at `position`, optionally carrying the button that was used.
pub const fn mouse(position: Point<CssPx, Css>, button: Option<PointerButton>) -> PointerEvent {
    PointerEvent {
        id: PointerId::MOUSE,
        kind: PointerKind::Mouse,
        primary: true,
        position,
        button,
        pressure: None,
    }
}

/// A finger, as the same kind of event a mouse produces.
///
/// The identifier the compositor hands out is kept, offset past the one reserved for the mouse, so
/// that two fingers down at once are two pointers rather than one that teleports.
pub fn touch(id: i32, position: Point<CssPx, Css>) -> PointerEvent {
    PointerEvent {
        // The mouse owns identifier zero, so every contact is numbered above it.
        id: PointerId::new(u64::from(id.unsigned_abs()).saturating_add(1)),
        kind: PointerKind::Touch,
        primary: id == 0,
        position,
        button: Some(PointerButton::Primary),
        pressure: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{mouse, position, touch};
    use zgui_geom::CssPx;
    use zgui_vocab::{PointerButton, PointerId, PointerKind};

    #[test]
    fn a_pointer_position_is_already_in_the_space_a_stylesheet_is_written_in() {
        // The compositor measures on the surface, not in the buffer. Dividing by the scale here
        // would place every click at a fraction of where it happened on a fractional display.
        let at = position(120.5, 64.25);
        assert_eq!(at.x, CssPx(120.5));
        assert_eq!(at.y, CssPx(64.25));
    }

    #[test]
    fn the_mouse_is_always_the_primary_pointer_and_always_the_same_one() {
        let at = position(0.0, 0.0);
        let event = mouse(at, Some(PointerButton::Primary));
        assert_eq!(event.id, PointerId::MOUSE);
        assert_eq!(event.kind, PointerKind::Mouse);
        assert!(event.primary);
    }

    #[test]
    fn two_fingers_down_at_once_are_two_pointers() {
        let at = position(0.0, 0.0);
        let first = touch(0, at);
        let second = touch(1, at);
        assert_ne!(first.id, second.id);
        assert!(first.primary, "the first contact leads the gesture");
        assert!(!second.primary);
    }

    #[test]
    fn no_contact_is_ever_numbered_as_the_mouse() {
        // A finger that collided with the mouse's identifier would make a drag teleport between
        // the two the moment both were down.
        for id in [0, 1, 7, i32::MAX] {
            assert_ne!(touch(id, position(0.0, 0.0)).id, PointerId::MOUSE);
        }
    }
}
