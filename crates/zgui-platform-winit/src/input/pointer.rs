//! A mouse, a finger and a stylus, as one stream told apart by a field.

use winit::dpi::PhysicalPosition;
use winit::event::{Force, MouseButton, Touch, TouchPhase};
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::{PointerAction, PointerButton, PointerEvent, PointerId, PointerKind};

/// Where a pointer is, in the space a layout is written in.
///
/// The platform reports physical pixels because that is what the hardware measures in; everything
/// above is written in CSS pixels because that is what a stylesheet is written in. The surface's
/// own scale is the bridge, and it is the surface's rather than the output's: a window can be
/// presented at a scale its monitor is not.
pub(crate) fn position(position: PhysicalPosition<f64>, scale_factor: f64) -> Point<CssPx, Css> {
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    Point::new(
        CssPx((position.x / scale) as f32),
        CssPx((position.y / scale) as f32),
    )
}

/// Which button was used.
///
/// A button the platform reports only as a number keeps that number rather than collapsing into
/// the primary one, because a mouse with eight buttons is a mouse someone bound all eight of.
pub(crate) const fn button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Back,
        MouseButton::Forward => PointerButton::Forward,
        MouseButton::Other(number) => PointerButton::Other(number),
    }
}

/// A mouse at `position`, optionally carrying the button that was used.
pub(crate) fn mouse(position: Point<CssPx, Css>, button: Option<PointerButton>) -> PointerEvent {
    PointerEvent {
        id: PointerId::MOUSE,
        kind: PointerKind::Mouse,
        primary: true,
        position,
        button,
        pressure: None,
    }
}

/// A touch or a stylus, as the same kind of event a mouse produces.
///
/// The identifier the platform hands out is kept, offset past the one reserved for the mouse, so
/// that two fingers down at once are two pointers rather than one that teleports. Pressure is
/// carried where the device reports it and is absent where it does not, which is not the same as
/// zero: a stylus resting on the glass reports a small pressure, and a finger reports none at all.
pub(crate) fn touch(touch: &Touch, scale_factor: f64) -> PointerEvent {
    PointerEvent {
        // The mouse owns identifier zero, so every contact is numbered above it.
        id: PointerId::new(touch.id.saturating_add(1)),
        kind: PointerKind::Touch,
        primary: touch.id == 0,
        position: position(touch.location, scale_factor),
        button: Some(PointerButton::Primary),
        pressure: touch.force.map(force),
    }
}

/// What a contact did.
pub(crate) const fn action(phase: TouchPhase) -> PointerAction {
    match phase {
        TouchPhase::Started => PointerAction::Pressed,
        TouchPhase::Moved => PointerAction::Moved,
        TouchPhase::Ended => PointerAction::Released,
        TouchPhase::Cancelled => PointerAction::Cancelled,
    }
}

/// How hard a contact is pressing, normalised to the nought-to-one range everything above uses.
fn force(force: Force) -> f32 {
    force.normalized() as f32
}

#[cfg(test)]
mod tests {
    use super::{action, button, mouse, position, touch};
    use winit::dpi::PhysicalPosition;
    use winit::event::{Force, MouseButton, Touch, TouchPhase};
    use zgui_geom::{CssPx, Point};
    use zgui_vocab::{PointerAction, PointerButton, PointerId, PointerKind};

    #[test]
    fn a_position_crosses_out_of_physical_pixels_by_the_surfaces_own_scale() {
        let at = position(PhysicalPosition::new(200.0, 100.0), 2.0);
        assert_eq!(at, Point::new(CssPx(100.0), CssPx(50.0)));
    }

    #[test]
    fn a_scale_of_zero_is_treated_as_one_rather_than_producing_infinities() {
        // A compositor answering zero is not a reason to place every pointer at infinity, which is
        // what dividing would do and what nothing downstream would survive.
        let at = position(PhysicalPosition::new(8.0, 8.0), 0.0);
        assert_eq!(at, Point::new(CssPx(8.0), CssPx(8.0)));
    }

    #[test]
    fn every_button_crosses_to_its_own_button() {
        let pairs = [
            (MouseButton::Left, PointerButton::Primary),
            (MouseButton::Right, PointerButton::Secondary),
            (MouseButton::Middle, PointerButton::Middle),
            (MouseButton::Back, PointerButton::Back),
            (MouseButton::Forward, PointerButton::Forward),
            (MouseButton::Other(9), PointerButton::Other(9)),
        ];
        for (platform, standard) in pairs {
            assert_eq!(button(platform), standard, "{platform:?} crossed wrongly");
        }
    }

    #[test]
    fn a_mouse_is_the_primary_pointer_and_owns_the_reserved_identifier() {
        let event = mouse(Point::new(CssPx(0.0), CssPx(0.0)), None);
        assert_eq!(event.id, PointerId::MOUSE);
        assert_eq!(event.kind, PointerKind::Mouse);
        assert!(event.primary);
    }

    #[test]
    fn two_fingers_are_two_pointers_and_neither_is_the_mouse() {
        let first = Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase: TouchPhase::Started,
            location: PhysicalPosition::new(10.0, 10.0),
            force: Some(Force::Normalized(0.5)),
            id: 0,
        };
        let second = Touch { id: 1, ..first };

        let first = touch(&first, 1.0);
        let second = touch(&second, 1.0);
        assert_ne!(first.id, second.id);
        assert_ne!(first.id, PointerId::MOUSE);
        assert_ne!(second.id, PointerId::MOUSE);
        assert_eq!(first.kind, PointerKind::Touch);
        assert!(first.primary, "the first contact is the primary one");
        assert!(!second.primary);
        assert_eq!(first.pressure, Some(0.5));
    }

    #[test]
    fn a_contact_that_never_reports_pressure_reports_none_rather_than_zero() {
        // None and zero mean different things to a control that varies a stroke by pressure: one is
        // "this device cannot say" and the other is "the pen is not touching the glass".
        let contact = Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase: TouchPhase::Moved,
            location: PhysicalPosition::new(0.0, 0.0),
            force: None,
            id: 3,
        };
        assert_eq!(touch(&contact, 1.0).pressure, None);
    }

    #[test]
    fn a_cancelled_contact_is_cancelled_and_not_released() {
        // A control that treated a cancel as a release would fire on a gesture the system took
        // away, which is the difference between a button that works and one that fires by accident.
        assert_eq!(action(TouchPhase::Cancelled), PointerAction::Cancelled);
        assert_eq!(action(TouchPhase::Ended), PointerAction::Released);
        assert_eq!(action(TouchPhase::Started), PointerAction::Pressed);
        assert_eq!(action(TouchPhase::Moved), PointerAction::Moved);
    }
}
