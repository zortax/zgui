//! The device pixel.

use core::fmt::{self, Display};

use crate::unit::{CssPx, Unit, length_ops};

/// A length in physical pixels on the output surface.
///
/// Everything the renderer consumes is measured in this unit, because this is the unit the pixel
/// grid is defined in: an edge at `DevicePx(10.0)` lands exactly on a pixel boundary, and one at
/// `DevicePx(10.3)` does not and will be antialiased.
///
/// A value is normally produced from [`CssPx`] by multiplying by a
/// [`Scale<Css, Device>`](crate::Scale), and geometry that has to look crisp goes through
/// [`snap_bounds`](crate::snap_bounds) or [`cover_bounds`](crate::cover_bounds) rather than
/// scaling directly, so that layout and the renderer agree on which pixel an edge is on.
///
/// ```
/// use zgui_geom::{Css, CssPx, Device, DevicePx, Scale};
///
/// let scale: Scale<Css, Device> = Scale::new(1.5);
/// assert_eq!(scale.apply_length(CssPx(10.0)), DevicePx(15.0));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct DevicePx(pub f32);

impl DevicePx {
    /// A zero-length value.
    pub const ZERO: Self = Self(0.0);

    /// One device pixel.
    pub const ONE: Self = Self(1.0);

    /// Reinterprets this length as a CSS pixel length, applying no scale.
    ///
    /// This is the identity on the underlying number and is only correct where the device pixel
    /// ratio is known to be 1, or where the value is a ratio rather than a position. Use
    /// [`Scale::invert`](crate::Scale::invert) and
    /// [`Scale::apply_length`](crate::Scale::apply_length) otherwise.
    pub const fn as_css_px_unscaled(self) -> CssPx {
        CssPx(self.0)
    }

    /// Whether the length sits exactly on the device pixel grid.
    ///
    /// ```
    /// use zgui_geom::DevicePx;
    ///
    /// assert!(DevicePx(3.0).is_grid_aligned());
    /// assert!(!DevicePx(3.5).is_grid_aligned());
    /// ```
    pub fn is_grid_aligned(self) -> bool {
        self.0.fract() == 0.0
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

impl Unit for DevicePx {
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

impl Display for DevicePx {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}dp", self.0)
    }
}

impl From<f32> for DevicePx {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl From<DevicePx> for f32 {
    fn from(value: DevicePx) -> Self {
        value.0
    }
}

length_ops!(DevicePx, f32);

#[cfg(test)]
mod tests {
    use super::DevicePx;

    #[test]
    fn arithmetic_stays_in_the_unit() {
        assert_eq!(DevicePx(3.0) + DevicePx(4.0), DevicePx(7.0));
        assert_eq!(DevicePx(3.0) * 2.0, DevicePx(6.0));
        assert_eq!(DevicePx(6.0) / DevicePx(2.0), 3.0);
    }

    #[test]
    fn grid_alignment_is_about_the_fractional_part() {
        assert!(DevicePx(-4.0).is_grid_aligned());
        assert!(!DevicePx(-4.25).is_grid_aligned());
    }

    #[test]
    fn displays_with_its_own_suffix() {
        assert_eq!(DevicePx(4.0).to_string(), "4dp");
    }
}
