//! The scalar units lengths are measured in.
//!
//! Three units and one conversion factor:
//!
//! - [`CssPx`] — a CSS pixel, the unit the author writes and the unit style computes in.
//! - [`DevicePx`] — a physical pixel on the output surface.
//! - [`Au`] — an app unit, exactly 1/60 of a CSS pixel, for arithmetic that must not drift.
//! - [`Scale`] — a ratio between two [spaces](crate::space), such as a display's device pixel
//!   ratio.
//!
//! All three units implement [`Unit`], which is what lets [`Point`](crate::Point),
//! [`Size`](crate::Size) and [`Rect`](crate::Rect) be generic over them while still supporting
//! arithmetic. Plain [`f32`] and [`i32`] implement it too, so a rectangle of whole device pixels
//! is spelled `Rect<i32, Device>` without inventing a unit for it.

pub mod au;
pub mod css_px;
pub mod device_px;
pub mod scale;

use core::fmt::Debug;
use core::ops::{Add, Neg, Sub};

use bytemuck::Pod;

pub use crate::unit::au::Au;
pub use crate::unit::css_px::CssPx;
pub use crate::unit::device_px::DevicePx;
pub use crate::unit::scale::Scale;

/// A scalar a length can be measured in.
///
/// The bounds are what the geometry types need in order to be useful: values can be added,
/// subtracted, negated, compared and ordered, they have a zero, and they can be moved through
/// [`f32`] so that a transform or a scale factor can be applied to them.
///
/// Implemented for [`CssPx`], [`DevicePx`], [`Au`], [`f32`] and [`i32`].
pub trait Unit:
    Copy
    + Debug
    + Default
    + PartialEq
    + PartialOrd
    + Pod
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Neg<Output = Self>
    + 'static
{
    /// The additive identity.
    const ZERO: Self;

    /// The value one unit away from [`Unit::ZERO`].
    const ONE: Self;

    /// Converts from a plain [`f32`] count of this unit.
    ///
    /// Units that are not floating point round to nearest, ties away from zero, and saturate
    /// rather than wrap.
    fn from_f32(value: f32) -> Self;

    /// Converts to a plain [`f32`] count of this unit.
    fn to_f32(self) -> f32;

    /// The smaller of two values.
    fn min(self, other: Self) -> Self;

    /// The larger of two values.
    fn max(self, other: Self) -> Self;

    /// The value clamped to `low ..= high`.
    ///
    /// # Panics
    ///
    /// Panics if `low > high`.
    fn clamp(self, low: Self, high: Self) -> Self {
        assert!(low <= high, "clamp needs low <= high");
        self.max(low).min(high)
    }
}

impl Unit for f32 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;

    fn from_f32(value: f32) -> Self {
        value
    }

    fn to_f32(self) -> f32 {
        self
    }

    fn min(self, other: Self) -> Self {
        f32::min(self, other)
    }

    fn max(self, other: Self) -> Self {
        f32::max(self, other)
    }
}

impl Unit for i32 {
    const ZERO: Self = 0;
    const ONE: Self = 1;

    fn from_f32(value: f32) -> Self {
        crate::unit::round_saturating(value)
    }

    fn to_f32(self) -> f32 {
        self as f32
    }

    fn min(self, other: Self) -> Self {
        Ord::min(self, other)
    }

    fn max(self, other: Self) -> Self {
        Ord::max(self, other)
    }
}

/// Rounds to the nearest integer, ties away from zero, saturating at the [`i32`] bounds.
///
/// `as` casts of out-of-range or non-finite floats saturate to the integer bounds already; the
/// rounding is what this adds. NaN becomes zero, because every other choice makes a NaN in one
/// coordinate move geometry somewhere surprising.
pub(crate) fn round_saturating(value: f32) -> i32 {
    if value.is_nan() {
        0
    } else {
        value.round() as i32
    }
}

/// Rounds a double to the nearest integer, ties away from zero, saturating at the [`i32`] bounds.
///
/// This is [`round_saturating`] at the precision that can hold every [`i32`] exactly, which is
/// what integer lengths are scaled through so that a factor of one is genuinely the identity.
pub(crate) fn round_saturating_f64(value: f64) -> i32 {
    if value.is_nan() {
        0
    } else {
        value.round() as i32
    }
}

/// Generates the arithmetic a single-field length newtype needs.
///
/// The type, its documentation and its inherent methods stay in the module that owns it; only
/// the operator impls, which are identical for every length, come from here.
macro_rules! length_ops {
    ($name:ident, $inner:ty) => {
        impl ::core::ops::Add for $name {
            type Output = Self;

            fn add(self, other: Self) -> Self {
                Self(self.0 + other.0)
            }
        }

        impl ::core::ops::AddAssign for $name {
            fn add_assign(&mut self, other: Self) {
                self.0 += other.0;
            }
        }

        impl ::core::ops::Sub for $name {
            type Output = Self;

            fn sub(self, other: Self) -> Self {
                Self(self.0 - other.0)
            }
        }

        impl ::core::ops::SubAssign for $name {
            fn sub_assign(&mut self, other: Self) {
                self.0 -= other.0;
            }
        }

        impl ::core::ops::Neg for $name {
            type Output = Self;

            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        impl ::core::ops::Mul<f32> for $name {
            type Output = Self;

            fn mul(self, factor: f32) -> Self {
                Self::from_scaled(self.0, factor)
            }
        }

        impl ::core::ops::MulAssign<f32> for $name {
            fn mul_assign(&mut self, factor: f32) {
                *self = *self * factor;
            }
        }

        impl ::core::ops::Div<f32> for $name {
            type Output = Self;

            fn div(self, divisor: f32) -> Self {
                Self::from_divided(self.0, divisor)
            }
        }

        impl ::core::ops::Div for $name {
            type Output = f32;

            fn div(self, divisor: Self) -> f32 {
                $crate::unit::Unit::to_f32(self) / $crate::unit::Unit::to_f32(divisor)
            }
        }

        impl ::core::iter::Sum for $name {
            fn sum<I: Iterator<Item = Self>>(iterator: I) -> Self {
                iterator.fold(Self(<$inner as Default>::default()), ::core::ops::Add::add)
            }
        }
    };
}

pub(crate) use length_ops;

#[cfg(test)]
mod tests {
    use super::{Unit, round_saturating};

    #[test]
    fn rounding_saturates_instead_of_wrapping() {
        assert_eq!(round_saturating(0.5), 1);
        assert_eq!(round_saturating(-0.5), -1);
        assert_eq!(round_saturating(f32::INFINITY), i32::MAX);
        assert_eq!(round_saturating(f32::NEG_INFINITY), i32::MIN);
        assert_eq!(round_saturating(f32::NAN), 0);
        assert_eq!(round_saturating(1e30), i32::MAX);
    }

    #[test]
    fn clamp_orders_its_bounds() {
        assert_eq!(Unit::clamp(5.0_f32, 0.0, 3.0), 3.0);
        assert_eq!(Unit::clamp(-5_i32, 0, 3), 0);
    }
}
