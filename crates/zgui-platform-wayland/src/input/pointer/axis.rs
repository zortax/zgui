//! Scrolling, in whichever unit the device measures in.
//!
//! # There is no sign conversion here, and that was measured
//!
//! An axis event's positive direction is the one the scroll *offset* moves in: a wheel turned
//! towards the person reports a positive vertical value and reveals content further down. That is
//! the convention [`zgui_platform::scroll`] states for everything above this seam, so the two
//! already agree and the value crosses unchanged.
//!
//! It is worth saying out loud, because the neighbouring backend does negate. `winit` describes
//! the movement of the *content* rather than of the offset, so its own Wayland layer flips the
//! protocol's sign on the way in and `zgui-platform-winit` flips it back on the way out. Two
//! negations that cancel are easy to read as one.
//!
//! A negation added here fails nothing: the document moves, smoothly, by the right distance, in
//! the wrong direction.
//!
//! # The person's preference is applied before this
//!
//! Natural scrolling is a setting of the input stack: it is applied before the compositor sees the
//! axis event, so what arrives is already pointing the way the person asked for. Flipping it here
//! would override a desktop setting on every machine.

use smithay_client_toolkit::seat::pointer::AxisScroll;
use zgui_geom::{CssPx, Size};
use zgui_vocab::{ScrollDelta, ScrollPhase};

/// How many hundred-and-twentieths of a detent the protocol reports one detent as.
const STEP: f32 = 120.0;

/// How far a scroll asked to move, in the unit it was reported in.
///
/// The *unit* is kept rather than converted, and that is the point of this being a branch rather
/// than a multiplication. A notched wheel reports whole detents, and how far a detent is depends on
/// the used line height of the element being scrolled and on how many lines this desktop means by
/// one detent — neither of which is known here. A constant invented at this boundary would be wrong
/// for every element with another line height and invisible in every case.
///
/// A wheel is told from a continuous surface by whether it reported steps. High-resolution wheels
/// report hundred-and-twentieths of a detent and coarse ones report whole detents, and both are
/// lines; only a device that reported neither is measuring in pixels.
pub fn delta(horizontal: &AxisScroll, vertical: &AxisScroll) -> ScrollDelta {
    if stepped(horizontal) || stepped(vertical) {
        return ScrollDelta::Lines {
            x: steps(horizontal),
            y: steps(vertical),
        };
    }
    ScrollDelta::Pixels(Size::new(
        CssPx(horizontal.absolute as f32),
        CssPx(vertical.absolute as f32),
    ))
}

/// Whether this axis reported whole detents rather than a distance.
const fn stepped(axis: &AxisScroll) -> bool {
    axis.value120 != 0 || axis.discrete != 0
}

/// How many detents this axis turned, from whichever resolution it reported.
fn steps(axis: &AxisScroll) -> f32 {
    if axis.value120 != 0 {
        return axis.value120 as f32 / STEP;
    }
    axis.discrete as f32
}

/// Where in a gesture this scroll sits.
///
/// A notched wheel has no gesture at all and every notch is its own event, which is what
/// [`ScrollPhase::Discrete`] means. A trackpad has a beginning, a middle and an end, and the end is
/// what lets an overscroll spring back rather than staying stretched. The compositor states the
/// end outright; the beginning is inferred from a gesture that has not started yet, because the
/// protocol has no event for it.
pub const fn phase(continuous: bool, stopping: bool, gesturing: bool) -> ScrollPhase {
    if !continuous {
        return ScrollPhase::Discrete;
    }
    if stopping {
        return ScrollPhase::Ended;
    }
    if gesturing {
        ScrollPhase::Moved
    } else {
        ScrollPhase::Started
    }
}

#[cfg(test)]
mod tests {
    use super::{delta, phase};
    use smithay_client_toolkit::seat::pointer::AxisScroll;
    use zgui_geom::CssPx;
    use zgui_vocab::{ScrollDelta, ScrollPhase};

    fn nothing() -> AxisScroll {
        AxisScroll::default()
    }

    fn wheel(value120: i32) -> AxisScroll {
        AxisScroll {
            value120,
            absolute: f64::from(value120) / 120.0 * 15.0,
            ..AxisScroll::default()
        }
    }

    fn finger(pixels: f64) -> AxisScroll {
        AxisScroll {
            absolute: pixels,
            ..AxisScroll::default()
        }
    }

    #[test]
    fn a_detent_turned_towards_the_user_moves_the_offset_down() {
        // Which is the direction the protocol already reports it in, so the number crosses whole.
        // Measured against what every other application on this desktop does with the same event.
        let ScrollDelta::Lines { y, .. } = delta(&nothing(), &wheel(120)) else {
            panic!("a wheel reported something other than lines");
        };
        assert_eq!(y, 1.0);
    }

    #[test]
    fn a_high_resolution_wheel_reports_fractions_of_a_detent_as_fractions_of_a_line() {
        let ScrollDelta::Lines { y, .. } = delta(&nothing(), &wheel(30)) else {
            panic!("a wheel reported something other than lines");
        };
        assert_eq!(y, 0.25);
    }

    #[test]
    fn a_coarse_wheel_that_reports_only_whole_detents_is_still_a_wheel() {
        let coarse = AxisScroll {
            discrete: 1,
            absolute: 15.0,
            ..AxisScroll::default()
        };
        assert_eq!(
            delta(&nothing(), &coarse),
            ScrollDelta::Lines { x: 0.0, y: 1.0 }
        );
    }

    #[test]
    fn a_touch_surface_reports_a_distance_and_it_stays_a_distance() {
        // Converting it to lines here would need a line height nothing at this boundary knows.
        let ScrollDelta::Pixels(moved) = delta(&finger(3.0), &finger(-12.5)) else {
            panic!("a finger reported something other than pixels");
        };
        assert_eq!(moved.width, CssPx(3.0));
        assert_eq!(moved.height, CssPx(-12.5));
    }

    #[test]
    fn a_wheel_has_no_gesture_around_it_and_a_finger_does() {
        assert_eq!(phase(false, false, false), ScrollPhase::Discrete);
        assert_eq!(phase(true, false, false), ScrollPhase::Started);
        assert_eq!(phase(true, false, true), ScrollPhase::Moved);
        assert_eq!(phase(true, true, true), ScrollPhase::Ended);
    }

    #[test]
    fn a_gesture_that_stops_before_it_moved_still_ends() {
        // Without the end an overscroll stays stretched rather than springing back.
        assert_eq!(phase(true, true, false), ScrollPhase::Ended);
    }
}
