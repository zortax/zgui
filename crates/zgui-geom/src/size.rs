//! A two-dimensional extent in a coordinate space.

use core::marker::PhantomData;
use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::space::Space;
use crate::space::derive::space_derives;
use crate::unit::Unit;

/// A width and a height measured in unit `T`, in coordinate space `S`.
///
/// A size doubles as the displacement between two positions, which is what
/// [`Point::offset_from`](crate::Point::offset_from) returns and what
/// [`Point`](crate::Point) can be moved by. Like every type here it carries the space it was
/// measured in, so a size in device pixels cannot be added to a position in CSS pixels.
///
/// ```
/// use zgui_geom::{Css, CssPx, Size};
///
/// let size: Size<CssPx, Css> = Size::new(CssPx(120.0), CssPx(40.0));
/// assert_eq!(size.area(), 4_800.0);
/// assert!(!size.is_empty());
/// ```
#[repr(C)]
pub struct Size<T, S> {
    /// The horizontal extent.
    pub width: T,
    /// The vertical extent.
    pub height: T,
    /// The coordinate space this extent was measured in.
    pub(crate) space: PhantomData<S>,
}

space_derives!(Size { width, height });

impl<T, S> Size<T, S> {
    /// An extent with the given width and height.
    pub const fn new(width: T, height: T) -> Self {
        Self {
            width,
            height,
            space: PhantomData,
        }
    }

    /// Applies a function to both extents, possibly changing the unit.
    pub fn map<U>(self, mut function: impl FnMut(T) -> U) -> Size<U, S> {
        Size::new(function(self.width), function(self.height))
    }

    /// Reinterprets the extent as belonging to a different space, keeping the numbers.
    ///
    /// This is an assertion by the caller that the two spaces coincide here. Anything that has to
    /// be converted goes through a [`Scale`](crate::Scale) instead.
    pub fn cast_space<D>(self) -> Size<T, D> {
        Size::new(self.width, self.height)
    }
}

impl<T: Unit, S: Space> Size<T, S> {
    /// An extent of zero in both directions.
    pub const ZERO: Self = Self {
        width: T::ZERO,
        height: T::ZERO,
        space: PhantomData,
    };

    /// The same extent in both directions.
    pub fn square(side: T) -> Self {
        Self::new(side, side)
    }

    /// Whether either extent is zero or negative, so the area is not positive.
    pub fn is_empty(self) -> bool {
        self.width <= T::ZERO || self.height <= T::ZERO
    }

    /// The product of the two extents, in squared units.
    pub fn area(self) -> f32 {
        self.width.to_f32() * self.height.to_f32()
    }

    /// The componentwise minimum of two extents.
    pub fn min(self, other: Self) -> Self {
        Self::new(self.width.min(other.width), self.height.min(other.height))
    }

    /// The componentwise maximum of two extents.
    pub fn max(self, other: Self) -> Self {
        Self::new(self.width.max(other.width), self.height.max(other.height))
    }

    /// The extent with both components clamped to at least zero.
    pub fn non_negative(self) -> Self {
        Self::new(self.width.max(T::ZERO), self.height.max(T::ZERO))
    }

    /// The extent with both components converted to another unit through [`f32`].
    pub fn to_unit<U: Unit>(self) -> Size<U, S> {
        Size::new(
            U::from_f32(self.width.to_f32()),
            U::from_f32(self.height.to_f32()),
        )
    }
}

impl<T: Unit, S: Space> Add for Size<T, S> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.width + other.width, self.height + other.height)
    }
}

impl<T: Unit, S: Space> AddAssign for Size<T, S> {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl<T: Unit, S: Space> Sub for Size<T, S> {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.width - other.width, self.height - other.height)
    }
}

impl<T: Unit, S: Space> SubAssign for Size<T, S> {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

#[cfg(test)]
mod tests {
    use super::Size;
    use crate::space::Css;
    use crate::unit::CssPx;

    #[test]
    fn emptiness_covers_zero_and_negative_extents() {
        let empty: Size<CssPx, Css> = Size::new(CssPx(0.0), CssPx(10.0));
        assert!(empty.is_empty());
        assert!(Size::<CssPx, Css>::new(CssPx(-1.0), CssPx(10.0)).is_empty());
        assert!(!Size::<CssPx, Css>::new(CssPx(1.0), CssPx(10.0)).is_empty());
    }

    #[test]
    fn negative_extents_clamp_to_zero() {
        let size: Size<CssPx, Css> = Size::new(CssPx(-4.0), CssPx(6.0));
        assert_eq!(size.non_negative(), Size::new(CssPx(0.0), CssPx(6.0)));
    }

    #[test]
    fn extents_add_componentwise() {
        let a: Size<CssPx, Css> = Size::new(CssPx(1.0), CssPx(2.0));
        assert_eq!(a + a, Size::new(CssPx(2.0), CssPx(4.0)));
    }
}
