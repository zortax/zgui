//! Elliptical corner radii.

use core::ops::{Add, Mul, Sub};

use crate::corners::Corners;
use crate::size::Size;
use crate::space::Space;
use crate::unit::Unit;

/// A pair of values along the two axes: a horizontal one and a vertical one.
///
/// As a corner radius this is the pair of semi-axes of the ellipse the corner is a quadrant of,
/// which is what `border-radius: 20px / 10px` specifies. [`Vec2::splat`] builds the circular case.
///
/// Unlike [`Point`](crate::Point) and [`Size`] it carries no coordinate space; a
/// radius is a length, and the same radii apply to a box wherever that box has been placed.
///
/// ```
/// use zgui_geom::{CssPx, Vec2};
///
/// let elliptical = Vec2::new(CssPx(20.0), CssPx(10.0));
/// let circular = Vec2::splat(CssPx(10.0));
/// assert!(!elliptical.is_square());
/// assert!(circular.is_square());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Vec2<T> {
    /// The horizontal component.
    pub x: T,
    /// The vertical component.
    pub y: T,
}

impl<T> Vec2<T> {
    /// A pair with the given components.
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }

    /// Applies a function to both components, possibly changing the value type.
    pub fn map<U>(self, mut function: impl FnMut(T) -> U) -> Vec2<U> {
        Vec2 {
            x: function(self.x),
            y: function(self.y),
        }
    }

    /// The two components, horizontal first.
    pub fn into_array(self) -> [T; 2] {
        [self.x, self.y]
    }
}

impl<T: Copy> Vec2<T> {
    /// The same value on both axes, which for a corner radius means a circular corner.
    pub const fn splat(value: T) -> Self {
        Self { x: value, y: value }
    }
}

impl<T: Unit> Vec2<T> {
    /// Zero on both axes.
    pub const ZERO: Self = Self::splat(T::ZERO);

    /// Whether both components are equal, so a corner radius describes a circular arc.
    ///
    /// Renderers use this to take a cheaper path; it is not a promise about how the corner is
    /// drawn.
    pub fn is_square(self) -> bool {
        self.x == self.y
    }

    /// Whether either component is zero or negative, so the corner is square rather than rounded.
    pub fn is_degenerate(self) -> bool {
        self.x <= T::ZERO || self.y <= T::ZERO
    }

    /// The components with both axes converted to another unit through [`f32`].
    pub fn to_unit<U: Unit>(self) -> Vec2<U> {
        Vec2::new(U::from_f32(self.x.to_f32()), U::from_f32(self.y.to_f32()))
    }
}

impl<T: Unit> Add for Vec2<T> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl<T: Unit> Sub for Vec2<T> {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl<T: Unit> Mul<f32> for Vec2<T> {
    type Output = Self;

    fn mul(self, factor: f32) -> Self {
        self.map(|value| T::from_f32(value.to_f32() * factor))
    }
}

impl<T: Unit> Corners<Vec2<T>> {
    /// Whether every corner is circular, that is has equal horizontal and vertical radii.
    pub fn is_circular(self) -> bool {
        self.all(|radius| radius.is_square())
    }

    /// Whether every corner radius is degenerate, so the box has square corners.
    pub fn is_square(self) -> bool {
        self.all(|radius| radius.is_degenerate())
    }

    /// The radii shrunk uniformly until no pair of adjacent corners overflows the box.
    ///
    /// Author-specified radii can be larger than the box they round. CSS resolves that by finding
    /// the smallest ratio `size / (sum of the two radii)` over the four sides and, if it is below
    /// one, multiplying *every* radius by it — uniformly, so the corners keep their relative
    /// proportions and adjacent curves meet exactly rather than crossing.
    ///
    /// Layout and the renderer must agree on the result exactly or a border and its background
    /// will disagree about where a curve begins, so this is the one place that computation lives.
    ///
    /// The result is never negative: a negative radius is meaningless, and a box with a negative
    /// extent — which layout can produce before it is clamped — collapses the radii on that axis
    /// to zero rather than turning them inside out.
    ///
    /// ```
    /// use zgui_geom::{Corners, Css, CssPx, Size, Vec2};
    ///
    /// // Two 60px radii along a 100px edge overflow it, so everything shrinks by 100/120.
    /// let radii = Corners::uniform(Vec2::splat(CssPx(60.0)));
    /// let box_size: Size<CssPx, Css> = Size::new(CssPx(100.0), CssPx(100.0));
    /// let fitted = radii.fit_within(box_size);
    /// assert_eq!(fitted.top_left, Vec2::splat(CssPx(50.0)));
    /// ```
    pub fn fit_within<S: Space>(self, size: Size<T, S>) -> Self {
        let width = size.width.to_f32();
        let height = size.height.to_f32();
        let clamped = self.map(|radius| Vec2::new(radius.x.max(T::ZERO), radius.y.max(T::ZERO)));

        let ratio = side_ratio(width, clamped.top_left.x, clamped.top_right.x)
            .min(side_ratio(
                height,
                clamped.top_right.y,
                clamped.bottom_right.y,
            ))
            .min(side_ratio(
                width,
                clamped.bottom_right.x,
                clamped.bottom_left.x,
            ))
            .min(side_ratio(
                height,
                clamped.bottom_left.y,
                clamped.top_left.y,
            ));

        // A ratio that is not below one leaves the radii alone, which is also the right answer for
        // a ratio that is not a number at all.
        if ratio.is_nan() || ratio >= 1.0 {
            clamped
        } else {
            clamped.map(|radius| radius * ratio)
        }
    }
}

/// How much of one side's length the two radii along it leave over, as a ratio.
///
/// A side no radius reaches imposes no limit, which is what the infinite result means. A side of
/// negative length leaves nothing over, so its radii collapse rather than reversing.
fn side_ratio<T: Unit>(side: f32, first: T, second: T) -> f32 {
    let total = first.to_f32() + second.to_f32();
    if total <= 0.0 {
        f32::INFINITY
    } else {
        (side / total).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::Vec2;
    use crate::corners::Corners;
    use crate::size::Size;
    use crate::space::Css;
    use crate::unit::CssPx;

    fn size(width: f32, height: f32) -> Size<CssPx, Css> {
        Size::new(CssPx(width), CssPx(height))
    }

    #[test]
    fn an_elliptical_corner_is_not_a_circular_one() {
        let radii = Corners::uniform(Vec2::new(CssPx(20.0), CssPx(10.0)));
        assert!(!radii.is_circular());
        assert!(!radii.is_square());
    }

    #[test]
    fn radii_that_fit_are_left_alone() {
        let radii = Corners::uniform(Vec2::new(CssPx(10.0), CssPx(5.0)));
        assert_eq!(radii.fit_within(size(100.0, 100.0)), radii);
    }

    #[test]
    fn the_tightest_side_decides_the_shrink_factor() {
        // The vertical radii sum to 200 over a 100px tall box, a ratio of 0.5; the horizontal
        // ones sum to 40 over 100, which is not binding.
        let radii = Corners::uniform(Vec2::new(CssPx(20.0), CssPx(100.0)));
        let fitted = radii.fit_within(size(100.0, 100.0));
        assert_eq!(fitted.top_left, Vec2::new(CssPx(10.0), CssPx(50.0)));
        assert_eq!(fitted.bottom_right, Vec2::new(CssPx(10.0), CssPx(50.0)));
    }

    #[test]
    fn shrinking_is_uniform_across_corners() {
        let radii = Corners::new(
            Vec2::splat(CssPx(80.0)),
            Vec2::splat(CssPx(20.0)),
            Vec2::splat(CssPx(0.0)),
            Vec2::splat(CssPx(0.0)),
        );
        let fitted = radii.fit_within(size(50.0, 50.0));
        // The top side is 100 wide against a 50px box, so every radius halves.
        assert_eq!(fitted.top_left, Vec2::splat(CssPx(40.0)));
        assert_eq!(fitted.top_right, Vec2::splat(CssPx(10.0)));
    }

    #[test]
    fn negative_radii_clamp_to_zero_before_fitting() {
        let radii = Corners::uniform(Vec2::new(CssPx(-5.0), CssPx(-5.0)));
        assert_eq!(
            radii.fit_within(size(10.0, 10.0)),
            Corners::uniform(Vec2::ZERO)
        );
    }

    #[test]
    fn a_box_with_a_negative_extent_collapses_the_radii() {
        let radii = Corners::uniform(Vec2::splat(CssPx(10.0)));
        let fitted = radii.fit_within(size(-20.0, 100.0));
        assert_eq!(fitted, Corners::uniform(Vec2::ZERO));
    }

    #[test]
    fn a_box_of_no_size_at_all_collapses_the_radii() {
        let radii = Corners::uniform(Vec2::splat(CssPx(10.0)));
        assert_eq!(
            radii.fit_within(size(0.0, 0.0)),
            Corners::uniform(Vec2::ZERO)
        );
    }

    #[test]
    fn fitted_radii_no_longer_overflow_the_box() {
        let radii = Corners::uniform(Vec2::new(CssPx(60.0), CssPx(90.0)));
        let fitted = radii.fit_within(size(100.0, 100.0));
        let epsilon = 1e-3;
        assert!(fitted.top_left.x.0 + fitted.top_right.x.0 <= 100.0 + epsilon);
        assert!(fitted.top_right.y.0 + fitted.bottom_right.y.0 <= 100.0 + epsilon);
        // Refitting moves nothing more than rounding accounts for.
        let again = fitted.fit_within(size(100.0, 100.0));
        assert!((again.top_left.x.0 - fitted.top_left.x.0).abs() < epsilon);
        assert!((again.top_left.y.0 - fitted.top_left.y.0).abs() < epsilon);
    }

    proptest! {
        /// Whatever it is given, fitting yields radii that are non-negative and that fit.
        #[test]
        fn fitted_radii_are_non_negative_and_fit_every_side(
            radii in prop::array::uniform8(-50.0_f32..200.0),
            width in -50.0_f32..300.0,
            height in -50.0_f32..300.0,
        ) {
            let corners = Corners::new(
                Vec2::new(CssPx(radii[0]), CssPx(radii[1])),
                Vec2::new(CssPx(radii[2]), CssPx(radii[3])),
                Vec2::new(CssPx(radii[4]), CssPx(radii[5])),
                Vec2::new(CssPx(radii[6]), CssPx(radii[7])),
            );
            let fitted = corners.fit_within(size(width, height));

            for radius in fitted.into_array() {
                prop_assert!(radius.x >= CssPx(0.0), "negative radius in {:?}", fitted);
                prop_assert!(radius.y >= CssPx(0.0), "negative radius in {:?}", fitted);
            }

            let usable_width = width.max(0.0);
            let usable_height = height.max(0.0);
            let epsilon = 1e-3 * usable_width.max(usable_height).max(1.0);
            prop_assert!(
                fitted.top_left.x.0 + fitted.top_right.x.0 <= usable_width + epsilon
            );
            prop_assert!(
                fitted.bottom_left.x.0 + fitted.bottom_right.x.0 <= usable_width + epsilon
            );
            prop_assert!(
                fitted.top_left.y.0 + fitted.bottom_left.y.0 <= usable_height + epsilon
            );
            prop_assert!(
                fitted.top_right.y.0 + fitted.bottom_right.y.0 <= usable_height + epsilon
            );
        }
    }
}
