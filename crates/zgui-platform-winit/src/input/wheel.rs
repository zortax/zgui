//! Scrolling, in whichever unit the device measures in, turned around to face the right way.
//!
//! # The one sign conversion in this backend
//!
//! `winit` and this framework describe a scroll from opposite ends. `MouseScrollDelta` documents
//! its positive direction as the one in which *the content being scrolled should move* — "positive
//! values indicate that the content that is being scrolled should move right and down (revealing
//! more content left and up)". A scroll offset moves the other way: revealing content further down
//! means a *larger* offset, and that is the convention
//! [`zgui_platform::scroll`] fixes for everything above this seam.
//!
//! So the two conventions differ by exactly one negation, and this is the only place in the
//! framework entitled to perform it. Left out, every list scrolls the way the wheel was not turned,
//! and nothing anywhere fails: the document moves, smoothly, by the right distance, in the wrong
//! direction — which reads as a missing preference rather than as a defect, and is the shape this
//! fault actually arrived in.
//!
//! Measured rather than deduced. On a Wayland session a detent pushed away from the user arrives
//! here as `LineDelta(0.0, 1.0)`, and every other application on that desktop responds to it by
//! moving *back* through its content. After the negation this backend reports `-1.0`, which the
//! scrolling system reads as "towards the top", which is what those applications did.
//!
//! # The person's preference is not applied here
//!
//! Natural scrolling is a setting of the input stack, not of the application: libinput applies it
//! before the compositor sees the axis event, so what arrives is already pointing the way the
//! person asked for. Flipping it again here would override a desktop setting on every machine —
//! which is why this file performs the *convention* conversion above and nothing else, and why
//! whether a preference still has to be applied is a question the backend answers through
//! [`ScrollDirection`](zgui_platform::scroll::ScrollDirection) rather than a constant in here.

use winit::event::{MouseScrollDelta, TouchPhase};
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_vocab::{PointerId, PointerKind, ScrollDelta, ScrollPhase, WheelEvent};

/// How far a scroll asked to move, in the unit it was reported in and in this framework's sign.
///
/// The *unit* is kept rather than converted, and that is the whole point of this function being a
/// match rather than one multiplication. A notched wheel reports whole detents, and how far a
/// detent is depends on the used line height of the element being scrolled and on how many lines
/// this desktop means by one detent — neither of which is known here, and neither of which can be.
/// A constant invented at this boundary would be wrong for every element with another line height
/// and for every desktop with another preference, and would be invisible in both cases.
///
/// Pixels are converted out of physical pixels into the space a layout is written in, because that
/// conversion needs only the surface's own scale and is exact.
pub(crate) fn delta(delta: MouseScrollDelta, scale_factor: f64) -> ScrollDelta {
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    match delta {
        MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines { x: -x, y: -y },
        MouseScrollDelta::PixelDelta(pixels) => ScrollDelta::Pixels(Size::new(
            CssPx(-(pixels.x / scale) as f32),
            CssPx(-(pixels.y / scale) as f32),
        )),
    }
}

/// Where in a gesture this scroll sits.
///
/// A notched wheel has no gesture at all and every notch is its own event, which is what
/// [`ScrollPhase::Discrete`] means. A trackpad has a beginning, a middle and an end, and the end is
/// what lets overscroll spring back rather than staying stretched.
///
/// Which of the two it is comes from the delta and not from the touch phase. A wheel reports whole
/// lines and a touch surface reports pixels, whereas the touch phase of a wheel notch is
/// [`TouchPhase::Moved`] on X11, Wayland and Win32 alike — the same value the middle of a trackpad
/// gesture carries. Reading the phase alone therefore calls every notch part of a gesture, and a
/// notch mistaken for a gesture is one nothing carries to its new place: the document arrives there
/// in a single frame.
pub(crate) const fn phase(delta: MouseScrollDelta, phase: TouchPhase) -> ScrollPhase {
    match delta {
        MouseScrollDelta::LineDelta(..) => ScrollPhase::Discrete,
        MouseScrollDelta::PixelDelta(_) => match phase {
            TouchPhase::Started => ScrollPhase::Started,
            TouchPhase::Moved => ScrollPhase::Moved,
            TouchPhase::Ended | TouchPhase::Cancelled => ScrollPhase::Ended,
        },
    }
}

/// One scroll, at the pointer's last known place.
///
/// A wheel turn carries no position of its own on any desktop protocol in use, so the position is
/// the one the pointer was last reported at. Without it a wheel turn could not be routed to
/// whatever is under the pointer, which is what a wheel turn means.
pub(crate) fn event(
    delta: ScrollDelta,
    phase: ScrollPhase,
    position: Point<CssPx, Css>,
) -> WheelEvent {
    WheelEvent {
        delta,
        phase,
        position,
        id: PointerId::MOUSE,
        kind: PointerKind::Mouse,
    }
}

#[cfg(test)]
mod tests {
    use super::{delta, phase};
    use winit::dpi::PhysicalPosition;
    use winit::event::{MouseScrollDelta, TouchPhase};
    use zgui_geom::{CssPx, Size};
    use zgui_vocab::{ScrollDelta, ScrollPhase};

    #[test]
    fn a_notch_stays_in_lines_and_is_never_guessed_into_pixels() {
        // A line height invented here would be wrong for every element that has another one, and
        // nothing downstream could tell that it had been invented. So would a count of lines per
        // detent, which is a property of the desktop rather than of this event.
        let three_notches = delta(MouseScrollDelta::LineDelta(0.0, -3.0), 2.0);
        assert!(three_notches.is_lines());
        assert_eq!(three_notches, ScrollDelta::Lines { x: 0.0, y: 3.0 });
    }

    #[test]
    fn a_swipe_crosses_out_of_physical_pixels_by_the_surfaces_own_scale() {
        let swipe = delta(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -27.0)),
            2.0,
        );
        assert_eq!(
            swipe,
            ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(13.5)))
        );
    }

    /// The measurement this backend's sign is set by.
    ///
    /// A detent pushed away from the user is `REL_WHEEL +1` at the kernel, which libinput and the
    /// compositor turn into an axis event that `winit` reports as `LineDelta(0.0, 1.0)`. Every
    /// other application on that desktop answers it by moving *back* through its content — three
    /// lines back, measured — so what leaves this backend has to be a negative block delta, which
    /// is what the scrolling system reads as "towards the top".
    #[test]
    fn a_detent_pushed_away_from_the_user_scrolls_towards_the_top() {
        let away = delta(MouseScrollDelta::LineDelta(0.0, 1.0), 1.0);
        assert_eq!(
            away,
            ScrollDelta::Lines { x: 0.0, y: -1.0 },
            "the wheel is turned around: the document would scroll the way the wheel was not turned"
        );

        let towards = delta(MouseScrollDelta::LineDelta(0.0, -1.0), 1.0);
        assert_eq!(towards, ScrollDelta::Lines { x: 0.0, y: 1.0 });
    }

    #[test]
    fn the_horizontal_axis_is_turned_around_with_the_vertical_one() {
        // A backend that converted one axis and not the other produces a document that scrolls
        // correctly down the page and backwards across it, which nobody notices until the first
        // horizontally scrolling table.
        let both = delta(MouseScrollDelta::LineDelta(2.0, 3.0), 1.0);
        assert_eq!(both, ScrollDelta::Lines { x: -2.0, y: -3.0 });
    }

    #[test]
    fn a_gesture_has_an_end_and_a_wheel_notch_does_not() {
        use winit::dpi::PhysicalPosition;

        // A notch is its own event whatever phase the platform stamps on it, and every desktop
        // stamps `Moved` — the same value the middle of a trackpad gesture carries.
        for stamped in [TouchPhase::Started, TouchPhase::Moved, TouchPhase::Ended] {
            assert_eq!(
                phase(MouseScrollDelta::LineDelta(0.0, 1.0), stamped),
                ScrollPhase::Discrete,
                "a whole detent is nobody's gesture, so something has to carry it"
            );
        }

        let pixels = |at| MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, at));
        assert_eq!(
            phase(pixels(1.0), TouchPhase::Started),
            ScrollPhase::Started
        );
        assert_eq!(phase(pixels(1.0), TouchPhase::Moved), ScrollPhase::Moved);
        assert_eq!(phase(pixels(1.0), TouchPhase::Ended), ScrollPhase::Ended);
        assert_eq!(
            phase(pixels(1.0), TouchPhase::Cancelled),
            ScrollPhase::Ended,
            "a cancelled gesture still has to let overscroll spring back"
        );
    }
}
