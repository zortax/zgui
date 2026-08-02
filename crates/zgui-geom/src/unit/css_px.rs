//! The CSS pixel.

use core::fmt::{self, Display};

use crate::unit::{Au, DevicePx, Unit, length_ops};

/// A length in CSS pixels.
///
/// This is the unit CSS is written in and the unit style computation produces: `width: 12px` is
/// `CssPx(12.0)`. It is device independent — the same document yields the same CSS pixel values
/// on a 1x display and a 3x one — so it says nothing about where a boundary falls on the physical
/// pixel grid. That question belongs to [`DevicePx`], reached through a
/// [`Scale<Css, Device>`](crate::Scale) and usually through
/// [`snap_bounds`](crate::snap_bounds).
///
/// ```
/// use zgui_geom::CssPx;
///
/// let width = CssPx(12.0);
/// assert_eq!(width * 2.0, CssPx(24.0));
/// assert_eq!(width + CssPx(0.5), CssPx(12.5));
/// ```
///
/// For arithmetic that repeats — summing a row of columns, distributing free space — prefer
/// [`Au`], which is exact, and convert once at the end.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct CssPx(pub f32);

impl CssPx {
    /// A zero-length value.
    pub const ZERO: Self = Self(0.0);

    /// One CSS pixel.
    pub const ONE: Self = Self(1.0);

    /// Converts to [`Au`], rounding to the nearest app unit.
    ///
    /// ```
    /// use zgui_geom::{Au, CssPx};
    ///
    /// assert_eq!(CssPx(1.0).to_au(), Au(60));
    /// assert_eq!(CssPx(0.5).to_au(), Au(30));
    /// ```
    pub fn to_au(self) -> Au {
        Au::from_css_px(self)
    }

    /// Reinterprets this length as a device pixel length, applying no scale.
    ///
    /// This is the identity on the underlying number and is only correct where the device pixel
    /// ratio is known to be 1, or where the value is a ratio rather than a position. Anything
    /// that has to land on the pixel grid goes through [`snap_bounds`](crate::snap_bounds) or
    /// [`cover_bounds`](crate::cover_bounds) instead.
    pub const fn as_device_px_unscaled(self) -> DevicePx {
        DevicePx(self.0)
    }

    /// The absolute value.
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// Whether the length is finite, that is neither infinite nor NaN.
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    /// Multiplies the raw value, for the generated [`Mul`](core::ops::Mul) impl.
    fn from_scaled(value: f32, factor: f32) -> Self {
        Self(value * factor)
    }

    /// Divides the raw value, for the generated [`Div`](core::ops::Div) impl.
    fn from_divided(value: f32, divisor: f32) -> Self {
        Self(value / divisor)
    }
}

impl Unit for CssPx {
    const ZERO: Self = Self::ZERO;
    const ONE: Self = Self::ONE;

    fn from_f32(value: f32) -> Self {
        Self(value)
    }

    fn to_f32(self) -> f32 {
        self.0
    }

    fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }
}

impl Display for CssPx {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}px", self.0)
    }
}

impl From<f32> for CssPx {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl From<CssPx> for f32 {
    fn from(value: CssPx) -> Self {
        value.0
    }
}

length_ops!(CssPx, f32);

#[cfg(test)]
mod tests {
    use super::CssPx;
    use crate::unit::Au;

    #[test]
    fn arithmetic_stays_in_the_unit() {
        assert_eq!(CssPx(3.0) + CssPx(4.0), CssPx(7.0));
        assert_eq!(CssPx(3.0) - CssPx(4.0), CssPx(-1.0));
        assert_eq!(-CssPx(3.0), CssPx(-3.0));
        assert_eq!(CssPx(3.0) * 2.0, CssPx(6.0));
        assert_eq!(CssPx(3.0) / 2.0, CssPx(1.5));
    }

    #[test]
    fn dividing_two_lengths_gives_a_ratio() {
        assert_eq!(CssPx(6.0) / CssPx(2.0), 3.0);
    }

    #[test]
    fn summing_an_empty_iterator_gives_zero() {
        let total: CssPx = [].into_iter().sum();
        assert_eq!(total, CssPx::ZERO);
    }

    #[test]
    fn round_trips_through_app_units() {
        assert_eq!(CssPx(1.0).to_au(), Au(60));
        assert_eq!(Au(90).to_css_px(), CssPx(1.5));
    }

    #[test]
    fn displays_with_its_css_suffix() {
        assert_eq!(CssPx(12.5).to_string(), "12.5px");
    }
}
