//! How far a tooltip's panel stands off the control it names.
//!
//! The panel is one of two things a tooltip draws. The other is the arrow, which reaches out of the
//! panel's edge towards the trigger — so a panel placed flush against the trigger puts the arrow
//! *inside* it. The numbers here keep the arrow's tip clear of the trigger, and they are the same
//! numbers the sheet turns the arrow with.

/// The width of the arrow's square, in CSS pixels, before it is turned on its corner.
///
/// The sheet states the same length.
const ARROW_SQUARE: f32 = 10.0;

/// How far the arrow is sunk behind the panel's edge, in CSS pixels.
///
/// The sheet centres the arrow on the inner line of the panel's one-pixel edge, so the arrow is
/// sunk by the width of that edge.
const ARROW_SINK: f32 = 1.0;

/// How far the arrow's tip stands past the panel's edge, in CSS pixels.
///
/// A square turned on its corner is as tall as its diagonal, so half of that diagonal is what
/// stands out of the panel, less the part the arrow is sunk by.
pub const ARROW_REACH: f32 = ARROW_SQUARE * core::f32::consts::SQRT_2 / 2.0 - ARROW_SINK;

/// How much clear space stays between the arrow's tip and the trigger, in CSS pixels.
const TIP_GAP: f32 = 4.0;

/// How far a tooltip's panel sits off its trigger, in CSS pixels.
///
/// Room for the arrow and a clear gap, so the tip points at the control from outside it.
pub const DEFAULT_OFFSET: f32 = ARROW_REACH + TIP_GAP;

/// How far the panel is placed off the trigger, for a tooltip that draws an arrow or does not.
///
/// A tooltip with an arrow is never placed closer than the arrow is long: the arrow is drawn
/// outside the panel, so an offset under [`ARROW_REACH`] would lay the tip over the trigger.
pub(super) fn side_offset(offset: f32, arrow: bool) -> f32 {
    if arrow {
        offset.max(DEFAULT_OFFSET)
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::{ARROW_REACH, DEFAULT_OFFSET, side_offset};

    #[test]
    fn the_arrow_reaches_out_of_the_panel() {
        // Half the diagonal of a ten pixel square, less the one pixel it is sunk by.
        assert!((ARROW_REACH - 6.071_068).abs() < 1.0e-4);
    }

    #[test]
    fn the_default_offset_clears_the_arrow() {
        // The tip stands four pixels clear of the trigger.
        assert!((DEFAULT_OFFSET - ARROW_REACH - 4.0).abs() < 1.0e-4);
    }

    #[test]
    fn an_arrow_holds_the_panel_off_the_trigger() {
        assert!((side_offset(0.0, true) - DEFAULT_OFFSET).abs() < f32::EPSILON);
        assert!((side_offset(4.0, true) - DEFAULT_OFFSET).abs() < f32::EPSILON);
    }

    #[test]
    fn a_larger_offset_is_kept() {
        assert!((side_offset(24.0, true) - 24.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_tooltip_without_an_arrow_sits_where_it_was_asked_to() {
        assert!((side_offset(0.0, false) - 0.0).abs() < f32::EPSILON);
        assert!((side_offset(4.0, false) - 4.0).abs() < f32::EPSILON);
    }
}
