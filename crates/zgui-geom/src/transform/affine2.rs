//! The two-dimensional affine transform.

use core::ops::Mul;

use crate::point::Point;
use crate::rect::Rect;
use crate::space::Space;
use crate::transform::Matrix4;
use crate::unit::Unit;

/// A two-dimensional affine transform, stored as the six coefficients CSS `matrix()` names.
///
/// The mapping is
///
/// ```text
/// x' = a * x + c * y + tx
/// y' = b * x + d * y + ty
/// ```
///
/// so `a`, `b`, `c` and `d` are the linear part — scale, rotation, skew, reflection — and `tx`,
/// `ty` are the translation. Everything an affine transform can do keeps straight lines straight
/// and parallel lines parallel, which is why a transformed rectangle still has a meaningful
/// axis-aligned bounding box.
///
/// ```
/// use zgui_geom::{Affine2, Css, CssPx, Point};
///
/// let move_right = Affine2::translation(10.0, 0.0);
/// let double = Affine2::scale(2.0, 2.0);
///
/// let point: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(1.0));
/// // `then` reads in application order: move first, then scale.
/// assert_eq!(
///     move_right.then(double).transform_point(point),
///     Point::new(CssPx(22.0), CssPx(2.0)),
/// );
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2 {
    /// The `x` coefficient of the transformed `x`.
    pub a: f32,
    /// The `x` coefficient of the transformed `y`.
    pub b: f32,
    /// The `y` coefficient of the transformed `x`.
    pub c: f32,
    /// The `y` coefficient of the transformed `y`.
    pub d: f32,
    /// The horizontal translation.
    pub tx: f32,
    /// The vertical translation.
    pub ty: f32,
}

impl Affine2 {
    /// The transform that changes nothing.
    pub const IDENTITY: Self = Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    /// A transform from its six coefficients, in the order CSS `matrix()` writes them.
    pub const fn new(a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) -> Self {
        Self { a, b, c, d, tx, ty }
    }

    /// A pure translation.
    pub const fn translation(tx: f32, ty: f32) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, tx, ty)
    }

    /// A pure scale about the origin.
    pub const fn scale(x: f32, y: f32) -> Self {
        Self::new(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    /// A rotation about the origin, in radians, turning the x axis toward the y axis.
    ///
    /// The y axis grows downward, so a positive angle turns clockwise on screen.
    pub fn rotation(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(cos, sin, -sin, cos, 0.0, 0.0)
    }

    /// A skew about the origin, with both angles in radians.
    pub fn skew(x_radians: f32, y_radians: f32) -> Self {
        Self::new(1.0, y_radians.tan(), x_radians.tan(), 1.0, 0.0, 0.0)
    }

    /// This transform followed by `next`.
    ///
    /// ```
    /// use zgui_geom::Affine2;
    ///
    /// let identity = Affine2::translation(3.0, 4.0).then(Affine2::translation(-3.0, -4.0));
    /// assert_eq!(identity, Affine2::IDENTITY);
    /// ```
    pub fn then(self, next: Self) -> Self {
        Self::new(
            next.a * self.a + next.c * self.b,
            next.b * self.a + next.d * self.b,
            next.a * self.c + next.c * self.d,
            next.b * self.c + next.d * self.d,
            next.a * self.tx + next.c * self.ty + next.tx,
            next.b * self.tx + next.d * self.ty + next.ty,
        )
    }

    /// The determinant of the linear part, which is the factor areas are multiplied by.
    pub fn determinant(self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    /// The transform that undoes this one, or `None` when it collapses the plane.
    pub fn invert(self) -> Option<Self> {
        let determinant = self.determinant();
        if determinant == 0.0 || !determinant.is_finite() {
            return None;
        }
        let inverse = determinant.recip();
        let a = self.d * inverse;
        let b = -self.b * inverse;
        let c = -self.c * inverse;
        let d = self.a * inverse;
        Some(Self::new(
            a,
            b,
            c,
            d,
            -(a * self.tx + c * self.ty),
            -(b * self.tx + d * self.ty),
        ))
    }

    /// Applies the transform to a position, translation included.
    pub fn transform_point<T: Unit, S: Space>(self, point: Point<T, S>) -> Point<T, S> {
        let x = point.x.to_f32();
        let y = point.y.to_f32();
        Point::new(
            T::from_f32(self.a * x + self.c * y + self.tx),
            T::from_f32(self.b * x + self.d * y + self.ty),
        )
    }

    /// Applies only the linear part, which is what a direction or an offset needs.
    pub fn transform_vector<T: Unit>(self, x: T, y: T) -> (T, T) {
        let (x, y) = (x.to_f32(), y.to_f32());
        (
            T::from_f32(self.a * x + self.c * y),
            T::from_f32(self.b * x + self.d * y),
        )
    }

    /// The smallest axis-aligned rectangle containing the transformed rectangle.
    ///
    /// A rotated rectangle is not a rectangle, so this is the bound of its four transformed
    /// corners — exactly what a clip test or a damage region needs.
    ///
    /// ```
    /// use zgui_geom::{Affine2, Css, CssPx, Point, Rect, Size};
    ///
    /// let square: Rect<CssPx, Css> = Rect::new(
    ///     Point::new(CssPx(0.0), CssPx(0.0)),
    ///     Size::new(CssPx(10.0), CssPx(10.0)),
    /// );
    /// let turned = Affine2::rotation(std::f32::consts::FRAC_PI_4).transform_rect(square);
    /// assert!(turned.width().0 > CssPx(14.1).0);
    /// ```
    pub fn transform_rect<T: Unit, S: Space>(self, rect: Rect<T, S>) -> Rect<T, S> {
        let [first, second, third, fourth] =
            rect.corners().map(|corner| self.transform_point(corner));
        let min = first.min(second).min(third).min(fourth);
        let max = first.max(second).max(third).max(fourth);
        Rect::from_corners(min, max)
    }

    /// Whether the transform only moves things, leaving every length and every angle alone.
    ///
    /// This is the question anything that has already rasterised at one orientation has to ask
    /// before reusing that raster: a translation moves the pixels, and everything else would
    /// resample them.
    ///
    /// ```
    /// use zgui_geom::Affine2;
    ///
    /// assert!(Affine2::IDENTITY.is_translation());
    /// assert!(Affine2::translation(3.5, -2.0).is_translation());
    /// assert!(!Affine2::rotation(0.1).is_translation());
    /// assert!(!Affine2::scale(1.0, 2.0).is_translation());
    /// ```
    pub fn is_translation(self) -> bool {
        self.a == 1.0 && self.b == 0.0 && self.c == 0.0 && self.d == 1.0
    }

    /// The same transform as a 4x4 matrix, with the z axis left alone.
    pub fn to_matrix4(self) -> Matrix4 {
        Matrix4::from_columns([
            [self.a, self.b, 0.0, 0.0],
            [self.c, self.d, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [self.tx, self.ty, 0.0, 1.0],
        ])
    }
}

impl Default for Affine2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for Affine2 {
    type Output = Self;

    /// Composes two transforms in the usual matrix order: `self` is applied *after* `other`.
    ///
    /// [`Affine2::then`] reads in the order things happen and is usually clearer.
    fn mul(self, other: Self) -> Self {
        other.then(self)
    }
}

#[cfg(test)]
mod tests {
    use core::f32::consts::FRAC_PI_2;

    use super::Affine2;
    use crate::point::Point;
    use crate::space::Css;
    use crate::unit::CssPx;

    fn point(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    fn close(left: Point<CssPx, Css>, right: Point<CssPx, Css>) -> bool {
        (left.x.0 - right.x.0).abs() < 1e-4 && (left.y.0 - right.y.0).abs() < 1e-4
    }

    #[test]
    fn a_quarter_turn_takes_the_x_axis_to_the_y_axis() {
        let turned = Affine2::rotation(FRAC_PI_2).transform_point(point(1.0, 0.0));
        assert!(close(turned, point(0.0, 1.0)));
    }

    #[test]
    fn composition_applies_the_left_transform_first() {
        let compound = Affine2::translation(1.0, 0.0).then(Affine2::scale(10.0, 10.0));
        assert!(close(
            compound.transform_point(point(0.0, 0.0)),
            point(10.0, 0.0)
        ));
    }

    #[test]
    fn multiplication_is_composition_the_other_way_round() {
        let left = Affine2::translation(1.0, 2.0);
        let right = Affine2::scale(3.0, 4.0);
        assert_eq!(left * right, right.then(left));
    }

    #[test]
    fn a_transform_and_its_inverse_cancel() {
        let transform = Affine2::translation(5.0, -2.0)
            .then(Affine2::rotation(0.6))
            .then(Affine2::scale(2.0, 3.0));
        let inverse = transform.invert().expect("invertible");
        let round_tripped = inverse.transform_point(transform.transform_point(point(7.0, -9.0)));
        assert!(close(round_tripped, point(7.0, -9.0)));
    }

    #[test]
    fn a_collapsed_transform_has_no_inverse() {
        assert_eq!(Affine2::scale(0.0, 1.0).invert(), None);
    }

    #[test]
    fn the_translation_lands_in_the_last_matrix4_column() {
        let matrix = Affine2::translation(3.0, 4.0).to_matrix4();
        assert_eq!(matrix.columns[3], [3.0, 4.0, 0.0, 1.0]);
    }
}
