// DERIVED-FROM: the GPUI project, crates/gpui/src/window.rs and crates/gpui/src/util.rs (Apache-2.0)
// The device-pixel snapping policy in this module — rounding with ties toward zero, the
// floor-and-ceil covering rule, and the rule that a non-zero stroke never rounds away — is
// adapted from that work, licensed under the Apache License, Version 2.0, and has been modified
// to work over this crate's coordinate spaces and units.

//! The device-pixel snapping policy.
//!
//! A CSS pixel boundary rarely lands on a physical pixel boundary. Left alone, a one-pixel border
//! becomes a two-pixel grey smear, adjacent boxes develop seams, and a box that moves by a
//! fraction of a pixel shimmers. Snapping fixes that by moving edges onto the device pixel grid —
//! but only if everyone snaps the *same* way. Layout, painting and hit testing all resolve
//! geometry through the functions here so that they cannot disagree.
//!
//! Three rules, because three questions need different answers:
//!
//! - [`snap_bounds`] rounds each edge to the nearest device pixel. This is for geometry that is
//!   *drawn*: it keeps a box crisp and keeps its position honest to within half a pixel.
//! - [`cover_bounds`] floors the near edges and ceils the far ones, so the result is a superset of
//!   the input. This is for geometry that *bounds* something — a clip, a damage region, a
//!   scissor rectangle — where losing a fraction of a pixel means losing a pixel of content.
//! - [`snap_stroke`] rounds a stroke width but never to zero. A hairline border that rounds down
//!   to nothing disappears from the page, which is far more visible than being a little too thick.
//!
//! ```
//! use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Rect, Scale, Size, cover_bounds, snap_bounds};
//!
//! let scale: Scale<Css, Device> = Scale::new(2.0);
//! let rect: Rect<CssPx, Css> = Rect::new(
//!     Point::new(CssPx(10.1), CssPx(10.1)),
//!     Size::new(CssPx(20.0), CssPx(20.0)),
//! );
//!
//! // Snapping moves each edge to the nearest device pixel.
//! assert_eq!(snap_bounds(rect, scale).origin, Point::new(DevicePx(20.0), DevicePx(20.0)));
//! // Covering never loses a fraction of a pixel.
//! assert_eq!(cover_bounds(rect, scale).origin, Point::new(DevicePx(20.0), DevicePx(20.0)));
//! assert_eq!(cover_bounds(rect, scale).far_corner(), Point::new(DevicePx(61.0), DevicePx(61.0)));
//! ```

pub mod bounds;
pub mod stroke;

use crate::unit::DevicePx;

pub use crate::snap::bounds::{cover_bounds, cover_device_bounds, snap_bounds, snap_device_bounds};
pub use crate::snap::stroke::{snap_device_stroke, snap_edges, snap_stroke};

/// Rounds to the nearest integer, with exact halves going toward zero.
///
/// Ties have to break somewhere, and toward zero is the choice that keeps a shape and its mirror
/// image the same size: rounding both `2.5` and `-2.5` away from zero would make a box centred on
/// the origin one pixel wider than a box that is not.
///
/// ```
/// use zgui_geom::snap::round_half_toward_zero;
///
/// assert_eq!(round_half_toward_zero(2.5), 2.0);
/// assert_eq!(round_half_toward_zero(-2.5), -2.0);
/// assert_eq!(round_half_toward_zero(2.6), 3.0);
/// ```
///
/// A value that is already a whole number is returned unchanged, however large it is, and a
/// non-finite value is returned as it came in.
pub fn round_half_toward_zero(value: f32) -> f32 {
    // Splitting off the whole part first is what keeps this exact for large coordinates: beyond
    // 2^23 a single-precision float has no fractional part left to inspect, and comparing
    // `value.abs() - 0.5` against the grid would move an already-whole coordinate by a pixel.
    let whole = value.trunc();
    if !whole.is_finite() {
        return value;
    }
    let fraction = value - whole;
    if fraction.abs() > 0.5 {
        whole + fraction.signum()
    } else {
        whole
    }
}

/// Rounds a device-space length onto the pixel grid, ties toward zero.
pub fn snap_length(length: DevicePx) -> DevicePx {
    DevicePx(round_half_toward_zero(length.0))
}

/// Rounds a device-space length down onto the pixel grid.
pub fn floor_length(length: DevicePx) -> DevicePx {
    DevicePx(length.0.floor())
}

/// Rounds a device-space length up onto the pixel grid.
pub fn ceil_length(length: DevicePx) -> DevicePx {
    DevicePx(length.0.ceil())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{ceil_length, floor_length, round_half_toward_zero, snap_length};
    use crate::unit::DevicePx;

    #[test]
    fn halves_break_toward_zero() {
        assert_eq!(round_half_toward_zero(0.5), 0.0);
        assert_eq!(round_half_toward_zero(-0.5), -0.0);
        assert_eq!(round_half_toward_zero(1.5), 1.0);
        assert_eq!(round_half_toward_zero(-1.5), -1.0);
    }

    #[test]
    fn anything_past_a_half_rounds_away() {
        assert_eq!(round_half_toward_zero(1.500_001), 2.0);
        assert_eq!(round_half_toward_zero(-1.500_001), -2.0);
    }

    #[test]
    fn a_coordinate_too_large_to_have_a_fraction_does_not_move() {
        // Past 2^23 consecutive floats are a whole pixel apart, so every value is already on the
        // grid and rounding must be the identity rather than a one-pixel jump.
        for value in [8_388_609.0_f32, -8_388_609.0, 1e30, -1e30] {
            assert_eq!(round_half_toward_zero(value), value);
            assert_eq!(snap_length(DevicePx(value)), DevicePx(value));
        }
    }

    #[test]
    fn non_finite_lengths_pass_through() {
        assert_eq!(round_half_toward_zero(f32::INFINITY), f32::INFINITY);
        assert_eq!(round_half_toward_zero(f32::NEG_INFINITY), f32::NEG_INFINITY);
        assert!(round_half_toward_zero(f32::NAN).is_nan());
    }

    proptest! {
        /// Every rounding rule lands on the grid and stays there.
        #[test]
        fn rounding_is_idempotent(value in -1e9_f32..1e9) {
            let length = DevicePx(value);
            for rounded in [snap_length(length), floor_length(length), ceil_length(length)] {
                prop_assert!(rounded.is_grid_aligned());
                prop_assert_eq!(snap_length(rounded), rounded);
                prop_assert_eq!(floor_length(rounded), rounded);
                prop_assert_eq!(ceil_length(rounded), rounded);
            }
        }

        /// Flooring never moves up and ceiling never moves down.
        #[test]
        fn covering_brackets_the_value(value in -1e9_f32..1e9) {
            let length = DevicePx(value);
            prop_assert!(floor_length(length) <= length);
            prop_assert!(ceil_length(length) >= length);
        }
    }
}
