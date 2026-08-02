//! The app unit.

use core::fmt::{self, Display};

use crate::unit::{CssPx, Unit, length_ops, round_saturating, round_saturating_f64};

/// How many app units make one CSS pixel.
///
/// Sixty is divisible by 2, 3, 4, 5, 6, 10, 12, 15, 20 and 30, so the lengths that actually occur
/// in layout — halves, thirds and fifths of a pixel from percentages, borders and flex
/// distribution — are all exactly representable.
pub const AU_PER_PX: i32 = 60;

/// A length in app units: exactly 1/60 of a [`CssPx`], stored as a signed integer.
///
/// Layout adds, subtracts and distributes lengths constantly. In binary floating point the result
/// of that depends on the order the additions happened in, which shows up as a column one pixel
/// wider than its neighbour for no visible reason. Integer app units have no rounding error to
/// accumulate, so this is the unit layout arithmetic is carried out in; the result converts to
/// [`CssPx`] once, at the end.
///
/// ```
/// use zgui_geom::{Au, CssPx};
///
/// let third: Au = CssPx(1.0).to_au() / 3.0;
/// assert_eq!(third, Au(20));
/// assert_eq!(third + third + third, CssPx(1.0).to_au());
/// ```
///
/// # Exactness
///
/// [`Au::to_px`] and [`Au::from_px`] round-trip every representable value with no drift at all,
/// because [`f64`] has more precision than [`i32`] has values. The [`CssPx`] conversions round-trip
/// exactly for `|value| <= `[`Au::EXACT_CSS_PX_LIMIT`], which is about ±69,905 CSS pixels — far
/// outside any real coordinate — and beyond that they lose the low bits, because [`f32`] has only
/// 24 bits of significand.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Au(pub i32);

impl Au {
    /// A zero-length value.
    pub const ZERO: Self = Self(0);

    /// One app unit, that is 1/60 of a CSS pixel.
    pub const ONE: Self = Self(1);

    /// One CSS pixel.
    pub const ONE_PX: Self = Self(AU_PER_PX);

    /// The largest representable length.
    pub const MAX: Self = Self(i32::MAX);

    /// The smallest representable length.
    pub const MIN: Self = Self(i32::MIN);

    /// The largest magnitude whose [`CssPx`] round trip is exact.
    ///
    /// For any `value` with `value.0.abs() <= Au::EXACT_CSS_PX_LIMIT.0`,
    /// `value.to_css_px().to_au() == value`.
    pub const EXACT_CSS_PX_LIMIT: Self = Self(4_194_304);

    /// Converts from CSS pixels, rounding to the nearest app unit.
    ///
    /// Ties round away from zero, non-finite values saturate, and NaN becomes zero.
    ///
    /// ```
    /// use zgui_geom::{Au, CssPx};
    ///
    /// assert_eq!(Au::from_css_px(CssPx(2.0)), Au(120));
    /// assert_eq!(Au::from_css_px(CssPx(-0.25)), Au(-15));
    /// ```
    pub fn from_css_px(value: CssPx) -> Self {
        Self(round_saturating(value.0 * AU_PER_PX as f32))
    }

    /// Converts to CSS pixels.
    ///
    /// Exact for magnitudes up to [`Au::EXACT_CSS_PX_LIMIT`]; see the type's exactness note.
    ///
    /// ```
    /// use zgui_geom::{Au, CssPx};
    ///
    /// assert_eq!(Au(150).to_css_px(), CssPx(2.5));
    /// ```
    pub fn to_css_px(self) -> CssPx {
        CssPx(self.0 as f32 / AU_PER_PX as f32)
    }

    /// Converts from a count of CSS pixels held at double precision, rounding to nearest.
    ///
    /// Together with [`Au::to_px`] this round-trips every representable value exactly.
    pub fn from_px(pixels: f64) -> Self {
        // `as` saturates at the integer bounds, so an out-of-range length clamps instead of
        // wrapping into a coordinate on the opposite side of the origin.
        Self(round_saturating_f64(pixels * f64::from(AU_PER_PX)))
    }

    /// Converts to a count of CSS pixels held at double precision.
    ///
    /// ```
    /// use zgui_geom::Au;
    ///
    /// assert_eq!(Au(i32::MAX), Au::from_px(Au(i32::MAX).to_px()));
    /// ```
    pub fn to_px(self) -> f64 {
        f64::from(self.0) / f64::from(AU_PER_PX)
    }

    /// The largest whole CSS pixel not greater than this length.
    ///
    /// ```
    /// use zgui_geom::Au;
    ///
    /// assert_eq!(Au(90).floor_to_px(), Au(60));
    /// assert_eq!(Au(-90).floor_to_px(), Au(-120));
    /// ```
    pub const fn floor_to_px(self) -> Self {
        Self(self.0.div_euclid(AU_PER_PX) * AU_PER_PX)
    }

    /// The smallest whole CSS pixel not less than this length.
    ///
    /// ```
    /// use zgui_geom::Au;
    ///
    /// assert_eq!(Au(90).ceil_to_px(), Au(120));
    /// assert_eq!(Au(120).ceil_to_px(), Au(120));
    /// ```
    pub const fn ceil_to_px(self) -> Self {
        let floored = self.floor_to_px();
        if floored.0 == self.0 {
            floored
        } else {
            Self(floored.0 + AU_PER_PX)
        }
    }

    /// The absolute value, saturating rather than overflowing at [`Au::MIN`].
    pub const fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }

    /// Multiplies the raw value, for the generated [`Mul`](core::ops::Mul) impl.
    ///
    /// The product is formed at double precision, so no app unit is lost on the way in and the
    /// only rounding is the one that lands back on the integer grid.
    fn from_scaled(value: i32, factor: f32) -> Self {
        Self(round_saturating_f64(f64::from(value) * f64::from(factor)))
    }

    /// Divides the raw value, for the generated [`Div`](core::ops::Div) impl.
    fn from_divided(value: i32, divisor: f32) -> Self {
        Self(round_saturating_f64(f64::from(value) / f64::from(divisor)))
    }
}

impl Unit for Au {
    const ZERO: Self = Self::ZERO;
    const ONE: Self = Self::ONE;

    fn from_f32(value: f32) -> Self {
        Self(round_saturating(value))
    }

    fn to_f32(self) -> f32 {
        self.0 as f32
    }

    fn min(self, other: Self) -> Self {
        Self(Ord::min(self.0, other.0))
    }

    fn max(self, other: Self) -> Self {
        Self(Ord::max(self.0, other.0))
    }
}

impl Display for Au {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}au", self.0)
    }
}

impl From<CssPx> for Au {
    fn from(value: CssPx) -> Self {
        Self::from_css_px(value)
    }
}

impl From<Au> for CssPx {
    fn from(value: Au) -> Self {
        value.to_css_px()
    }
}

length_ops!(Au, i32);

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{AU_PER_PX, Au};
    use crate::unit::CssPx;

    #[test]
    fn thirds_of_a_pixel_are_exact() {
        let third = Au(AU_PER_PX / 3);
        assert_eq!(third + third + third, Au::ONE_PX);
    }

    #[test]
    fn conversion_rounds_to_nearest_away_from_zero() {
        assert_eq!(Au::from_css_px(CssPx(1.0 / 60.0 / 2.0)), Au(1));
        assert_eq!(Au::from_css_px(CssPx(-1.0 / 60.0 / 2.0)), Au(-1));
    }

    #[test]
    fn out_of_range_lengths_saturate_instead_of_wrapping() {
        assert_eq!(Au::from_px(f64::INFINITY), Au::MAX);
        assert_eq!(Au::from_px(f64::NEG_INFINITY), Au::MIN);
        assert_eq!(Au::from_px(f64::NAN), Au::ZERO);
        assert_eq!(Au::from_css_px(CssPx(f32::INFINITY)), Au::MAX);
    }

    #[test]
    fn scaling_by_one_is_the_identity_at_every_magnitude() {
        // The product is formed at double precision, so a length past the 24 bits an `f32`
        // significand holds is not quietly truncated on the way through.
        for raw in [20_000_001, 16_777_217, i32::MAX, i32::MIN + 1] {
            assert_eq!(Au(raw) * 1.0, Au(raw));
            assert_eq!(Au(raw) / 1.0, Au(raw));
        }
        assert_eq!(Au(20_000_002) / 2.0, Au(10_000_001));
    }

    #[test]
    fn whole_pixel_rounding_matches_its_documentation() {
        assert_eq!(Au(0).floor_to_px(), Au(0));
        assert_eq!(Au(0).ceil_to_px(), Au(0));
        assert_eq!(Au(-1).floor_to_px(), Au(-60));
        assert_eq!(Au(-1).ceil_to_px(), Au(0));
    }

    proptest! {
        /// The pixel round trip is exact for every value the type can hold.
        #[test]
        fn px_round_trip_is_exact_over_the_full_i32_range(raw in any::<i32>()) {
            let value = Au(raw);
            prop_assert_eq!(Au::from_px(value.to_px()), value);
        }

        /// Converting to pixels and back never drifts, however many times it is repeated.
        #[test]
        fn px_round_trip_never_drifts(raw in any::<i32>(), rounds in 1_usize..8) {
            let value = Au(raw);
            let mut moved = value;
            for _ in 0..rounds {
                moved = Au::from_px(moved.to_px());
            }
            prop_assert_eq!(moved, value);
        }

        /// The single-precision round trip is exact across the documented range.
        #[test]
        fn css_px_round_trip_is_exact_within_the_documented_limit(
            raw in -Au::EXACT_CSS_PX_LIMIT.0..=Au::EXACT_CSS_PX_LIMIT.0,
        ) {
            let value = Au(raw);
            prop_assert_eq!(value.to_css_px().to_au(), value);
        }

        /// Whichever direction the round trip starts in, one pass reaches a fixed point.
        #[test]
        fn css_px_round_trip_settles_after_one_pass(
            pixels in -70_000.0_f32..70_000.0,
        ) {
            let once = CssPx(pixels).to_au();
            prop_assert_eq!(once.to_css_px().to_au(), once);
        }
    }
}
