//! `position: sticky`, resolved as part of composing absolute geometry.
//!
//! A sticky box is laid out exactly where it would sit in the normal flow and is then *shifted* by
//! the fragment pass, never by layout: nothing around it moves, and no size changes, so re-running
//! layout for it would be work with no output. The shift is whatever it takes to keep the box
//! inside its own scrollport under the given insets, clamped so it never escapes its containing
//! block — which is what makes a sticky heading stop at the bottom of its own section instead of
//! following the scroll forever.

use zgui_css::ComputedStyle;
use zgui_css::values::size::{InsetValue, PositionValue};
use zgui_geom::{CssPx, Device, DevicePx, Rect};

/// How far a sticky box is shifted from where layout put it, in device pixels.
///
/// `flow` is the box's border box after the scroll offset has been applied, `scrollport` is the
/// visible rectangle it is sticky within, and `containing` is its containing block's content box —
/// also scrolled, so all three are in one space.
///
/// Returns a zero offset for every box that is not sticky, which is nearly all of them.
pub fn offset(
    style: &ComputedStyle,
    flow: Rect<DevicePx, Device>,
    scrollport: Rect<DevicePx, Device>,
    containing: Rect<DevicePx, Device>,
    scale: f32,
) -> (f32, f32) {
    if style.get_box().position != PositionValue::Sticky {
        return (0.0, 0.0);
    }
    let position = style.get_position();
    let horizontal = scrollport.size.width.0;
    let vertical = scrollport.size.height.0;
    let x = axis(
        inset(&position.left, horizontal, scale),
        inset(&position.right, horizontal, scale),
        flow.left().0,
        flow.right().0,
        scrollport.left().0,
        scrollport.right().0,
        containing.left().0,
        containing.right().0,
    );
    let y = axis(
        inset(&position.top, vertical, scale),
        inset(&position.bottom, vertical, scale),
        flow.top().0,
        flow.bottom().0,
        scrollport.top().0,
        scrollport.bottom().0,
        containing.top().0,
        containing.bottom().0,
    );
    (x, y)
}

/// The shift on one axis.
///
/// Both insets can be given at once, and CSS resolves that by applying the start inset first: a box
/// with `top` and `bottom` both set sticks to the top and is then pushed back up by the bottom
/// constraint only if it would otherwise fall out of the far edge. The final clamp is against the
/// containing block, and it is applied to the *shift* rather than to the position, so a box whose
/// containing block has already scrolled past simply stops moving.
#[allow(clippy::too_many_arguments)]
fn axis(
    start_inset: Option<f32>,
    end_inset: Option<f32>,
    flow_start: f32,
    flow_end: f32,
    port_start: f32,
    port_end: f32,
    containing_start: f32,
    containing_end: f32,
) -> f32 {
    let mut shift = 0.0;
    if let Some(inset) = start_inset {
        let wanted = port_start + inset;
        if flow_start < wanted {
            shift = wanted - flow_start;
        }
    }
    if let Some(inset) = end_inset {
        let wanted = port_end - inset;
        if flow_end + shift > wanted {
            shift = wanted - flow_end;
        }
    }
    if shift == 0.0 {
        return 0.0;
    }
    // Never past the containing block: the box may move within it and no further.
    let highest = containing_end - flow_end;
    let lowest = containing_start - flow_start;
    shift.clamp(lowest.min(0.0), highest.max(0.0))
}

/// One inset in device pixels, or nothing when it is `auto`.
fn inset(value: &InsetValue, basis: f32, scale: f32) -> Option<f32> {
    match value {
        InsetValue::Auto => None,
        InsetValue::LengthPercentage(length) => {
            Some(zgui_css::values::length::evaluate_at(length, CssPx(basis / scale)).0 * scale)
        }
        // An anchor-positioned inset resolves against an anchor this engine does not place.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::axis;

    #[test]
    fn a_box_above_its_inset_is_pushed_down_to_it() {
        // Flow position 0..20, scrollport 50..150, `top: 10` wants the box at 60.
        let shift = axis(Some(10.0), None, 0.0, 20.0, 50.0, 150.0, -1000.0, 1000.0);
        assert_eq!(shift, 60.0);
    }

    #[test]
    fn a_box_already_past_its_inset_does_not_move() {
        let shift = axis(Some(10.0), None, 80.0, 100.0, 50.0, 150.0, -1000.0, 1000.0);
        assert_eq!(shift, 0.0);
    }

    #[test]
    fn the_shift_stops_at_the_containing_block() {
        // The box wants to move down 60, but its containing block ends 30 below its own end.
        let shift = axis(Some(10.0), None, 0.0, 20.0, 50.0, 150.0, -1000.0, 50.0);
        assert_eq!(shift, 30.0);
    }

    #[test]
    fn an_end_inset_pushes_a_box_back_inside_the_far_edge() {
        // Flow 200..260, scrollport 0..150, `bottom: 10` wants the end at 140.
        let shift = axis(None, Some(10.0), 200.0, 260.0, 0.0, 150.0, -1000.0, 1000.0);
        assert_eq!(shift, -120.0);
    }
}
