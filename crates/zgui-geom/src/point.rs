//! A position in a coordinate space.

use core::marker::PhantomData;
use core::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use crate::size::Size;
use crate::space::Space;
use crate::space::derive::space_derives;
use crate::unit::Unit;

/// A position measured in unit `T`, in coordinate space `S`.
///
/// The space parameter costs nothing at runtime — it is a zero-sized marker — and buys the
/// guarantee that a position from one coordinate system cannot be used where another is expected:
///
/// ```compile_fail
/// # use zgui_geom::{Css, CssPx, Device, Point};
/// let css: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(2.0));
/// let device: Point<CssPx, Device> = Point::new(CssPx(3.0), CssPx(4.0));
/// let _ = css + device;
/// ```
///
/// Within one space the arithmetic is ordinary:
///
/// ```
/// use zgui_geom::{Css, CssPx, Point, Size};
///
/// let origin: Point<CssPx, Css> = Point::new(CssPx(10.0), CssPx(20.0));
/// let moved = origin + Size::new(CssPx(5.0), CssPx(0.0));
/// assert_eq!(moved, Point::new(CssPx(15.0), CssPx(20.0)));
/// ```
///
/// The axes point right and down: `x` grows to the right, `y` grows downward.
#[repr(C)]
pub struct Point<T, S> {
    /// The horizontal coordinate, growing to the right.
    pub x: T,
    /// The vertical coordinate, growing downward.
    pub y: T,
    /// The coordinate space this position was measured in.
    pub(crate) space: PhantomData<S>,
}

space_derives!(Point { x, y });

impl<T, S> Point<T, S> {
    /// A position at the given coordinates.
    pub const fn new(x: T, y: T) -> Self {
        Self {
            x,
            y,
            space: PhantomData,
        }
    }

    /// Applies a function to both coordinates, possibly changing the unit.
    ///
    /// The space is preserved, because mapping a coordinate does not move it to a different
    /// coordinate system.
    ///
    /// ```
    /// use zgui_geom::{Css, CssPx, Point};
    ///
    /// let point: Point<CssPx, Css> = Point::new(CssPx(1.5), CssPx(-2.5));
    /// assert_eq!(point.map(|value| value.abs()), Point::new(CssPx(1.5), CssPx(2.5)));
    /// ```
    pub fn map<U>(self, mut function: impl FnMut(T) -> U) -> Point<U, S> {
        Point::new(function(self.x), function(self.y))
    }

    /// Reinterprets the position as belonging to a different space, keeping the numbers.
    ///
    /// This is an assertion by the caller that the two spaces coincide here — typically because
    /// a scale of one relates them. Anything that has to be converted goes through a
    /// [`Scale`](crate::Scale) instead.
    pub fn cast_space<D>(self) -> Point<T, D> {
        Point::new(self.x, self.y)
    }
}

impl<T: Unit, S: Space> Point<T, S> {
    /// The origin of the coordinate space.
    pub const ORIGIN: Self = Self {
        x: T::ZERO,
        y: T::ZERO,
        space: PhantomData,
    };

    /// The offset from `other` to `self`, as a size.
    ///
    /// ```
    /// use zgui_geom::{Css, CssPx, Point, Size};
    ///
    /// let a: Point<CssPx, Css> = Point::new(CssPx(10.0), CssPx(10.0));
    /// let b = Point::new(CssPx(4.0), CssPx(3.0));
    /// assert_eq!(a.offset_from(b), Size::new(CssPx(6.0), CssPx(7.0)));
    /// ```
    pub fn offset_from(self, other: Self) -> Size<T, S> {
        Size::new(self.x - other.x, self.y - other.y)
    }

    /// The componentwise minimum of two positions.
    pub fn min(self, other: Self) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y))
    }

    /// The componentwise maximum of two positions.
    pub fn max(self, other: Self) -> Self {
        Self::new(self.x.max(other.x), self.y.max(other.y))
    }

    /// The position with both coordinates converted to another unit through [`f32`].
    ///
    /// ```
    /// use zgui_geom::{Au, Css, CssPx, Point};
    ///
    /// let point: Point<CssPx, Css> = Point::new(CssPx(2.0), CssPx(3.0));
    /// assert_eq!(point.to_unit::<f32>(), Point::new(2.0, 3.0));
    /// ```
    pub fn to_unit<U: Unit>(self) -> Point<U, S> {
        Point::new(U::from_f32(self.x.to_f32()), U::from_f32(self.y.to_f32()))
    }
}

impl<T: Unit, S: Space> Add<Self> for Point<T, S> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl<T: Unit, S: Space> AddAssign<Self> for Point<T, S> {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl<T: Unit, S: Space> Sub<Self> for Point<T, S> {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl<T: Unit, S: Space> SubAssign<Self> for Point<T, S> {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl<T: Unit, S: Space> Add<Size<T, S>> for Point<T, S> {
    type Output = Self;

    fn add(self, offset: Size<T, S>) -> Self {
        Self::new(self.x + offset.width, self.y + offset.height)
    }
}

impl<T: Unit, S: Space> Sub<Size<T, S>> for Point<T, S> {
    type Output = Self;

    fn sub(self, offset: Size<T, S>) -> Self {
        Self::new(self.x - offset.width, self.y - offset.height)
    }
}

impl<T: Unit, S: Space> Neg for Point<T, S> {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::Point;
    use crate::size::Size;
    use crate::space::{Css, Device};
    use crate::unit::CssPx;

    #[test]
    fn positions_add_and_subtract_within_a_space() {
        let a: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(2.0));
        let b = Point::new(CssPx(3.0), CssPx(5.0));
        assert_eq!(a + b, Point::new(CssPx(4.0), CssPx(7.0)));
        assert_eq!(b - a, Point::new(CssPx(2.0), CssPx(3.0)));
        assert_eq!(b.offset_from(a), Size::new(CssPx(2.0), CssPx(3.0)));
    }

    #[test]
    fn the_space_shows_up_in_the_debug_output() {
        let css: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(2.0));
        let device: Point<CssPx, Device> = css.cast_space();
        assert!(format!("{css:?}").contains("css"));
        assert!(format!("{device:?}").contains("device"));
    }

    #[test]
    fn equality_ignores_nothing_but_the_marker() {
        let a: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(2.0));
        assert_eq!(a, Point::new(CssPx(1.0), CssPx(2.0)));
        assert_ne!(a, Point::new(CssPx(1.0), CssPx(2.5)));
    }
}
