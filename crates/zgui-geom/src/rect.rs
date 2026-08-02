//! An axis-aligned rectangle in a coordinate space.

use crate::edges::Edges;
use crate::point::Point;
use crate::size::Size;
use crate::space::Space;
use crate::space::derive::space_derives;
use crate::unit::Unit;

/// An axis-aligned rectangle: an origin and an extent, measured in unit `T` and space `S`.
///
/// The origin is the top-left corner and the extent grows right and down, so the far edges are
/// `origin + size`. A rectangle whose extent is zero or negative on either axis is *empty*: it
/// contains no points, and intersecting with it yields nothing.
///
/// ```
/// use zgui_geom::{Css, CssPx, Point, Rect, Size};
///
/// let outer: Rect<CssPx, Css> = Rect::new(
///     Point::new(CssPx(0.0), CssPx(0.0)),
///     Size::new(CssPx(100.0), CssPx(50.0)),
/// );
/// let inner = Rect::new(
///     Point::new(CssPx(10.0), CssPx(10.0)),
///     Size::new(CssPx(20.0), CssPx(20.0)),
/// );
/// assert!(outer.contains_rect(inner));
/// assert_eq!(outer.intersection(inner), Some(inner));
/// ```
#[repr(C)]
pub struct Rect<T, S> {
    /// The top-left corner.
    pub origin: Point<T, S>,
    /// The extent, growing right and down from the origin.
    pub size: Size<T, S>,
}

space_derives!(Rect { origin, size } tagged by its fields);

impl<T, S> Rect<T, S> {
    /// A rectangle at `origin` with extent `size`.
    pub const fn new(origin: Point<T, S>, size: Size<T, S>) -> Self {
        Self { origin, size }
    }

    /// Reinterprets the rectangle as belonging to a different space, keeping the numbers.
    ///
    /// This is an assertion by the caller that the two spaces coincide here. Anything that has to
    /// be converted goes through a [`Scale`](crate::Scale) instead.
    pub fn cast_space<D>(self) -> Rect<T, D> {
        Rect::new(self.origin.cast_space(), self.size.cast_space())
    }
}

impl<T: Unit, S: Space> Rect<T, S> {
    /// The empty rectangle at the origin.
    pub const ZERO: Self = Self {
        origin: Point::ORIGIN,
        size: Size::ZERO,
    };

    /// The smallest rectangle with the two given corners, in either order.
    ///
    /// ```
    /// use zgui_geom::{Css, CssPx, Point, Rect, Size};
    ///
    /// let rect: Rect<CssPx, Css> = Rect::from_corners(
    ///     Point::new(CssPx(30.0), CssPx(10.0)),
    ///     Point::new(CssPx(10.0), CssPx(40.0)),
    /// );
    /// assert_eq!(rect.origin, Point::new(CssPx(10.0), CssPx(10.0)));
    /// assert_eq!(rect.size, Size::new(CssPx(20.0), CssPx(30.0)));
    /// ```
    pub fn from_corners(first: Point<T, S>, second: Point<T, S>) -> Self {
        let min = first.min(second);
        let max = first.max(second);
        Self::new(min, max.offset_from(min))
    }

    /// The left edge.
    pub fn left(self) -> T {
        self.origin.x
    }

    /// The top edge.
    pub fn top(self) -> T {
        self.origin.y
    }

    /// The right edge, that is the left edge plus the width.
    pub fn right(self) -> T {
        self.origin.x + self.size.width
    }

    /// The bottom edge, that is the top edge plus the height.
    pub fn bottom(self) -> T {
        self.origin.y + self.size.height
    }

    /// The width.
    pub fn width(self) -> T {
        self.size.width
    }

    /// The height.
    pub fn height(self) -> T {
        self.size.height
    }

    /// The bottom-right corner.
    pub fn far_corner(self) -> Point<T, S> {
        Point::new(self.right(), self.bottom())
    }

    /// The four corners, clockwise from the top left.
    pub fn corners(self) -> [Point<T, S>; 4] {
        [
            self.origin,
            Point::new(self.right(), self.top()),
            self.far_corner(),
            Point::new(self.left(), self.bottom()),
        ]
    }

    /// Whether the rectangle contains no points, because an extent is zero or negative.
    pub fn is_empty(self) -> bool {
        self.size.is_empty()
    }

    /// Whether `point` lies inside the rectangle, near edges included and far edges excluded.
    ///
    /// Half-open containment is what makes adjacent rectangles tile a plane without a point
    /// belonging to two of them.
    pub fn contains(self, point: Point<T, S>) -> bool {
        point.x >= self.left()
            && point.x < self.right()
            && point.y >= self.top()
            && point.y < self.bottom()
    }

    /// Whether `other` lies entirely within this rectangle.
    ///
    /// An empty rectangle is contained by anything, since it holds no points to fall outside.
    pub fn contains_rect(self, other: Rect<T, S>) -> bool {
        if other.is_empty() {
            return true;
        }
        !self.is_empty()
            && other.left() >= self.left()
            && other.top() >= self.top()
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    /// Whether the two rectangles share any point.
    pub fn intersects(self, other: Rect<T, S>) -> bool {
        self.intersection(other).is_some()
    }

    /// The overlap of the two rectangles, or `None` when they do not overlap.
    pub fn intersection(self, other: Rect<T, S>) -> Option<Self> {
        let origin = self.origin.max(other.origin);
        let far = self.far_corner().min(other.far_corner());
        let candidate = Self::new(origin, far.offset_from(origin));
        (far.x > origin.x && far.y > origin.y).then_some(candidate)
    }

    /// The smallest rectangle containing both, ignoring either if it is empty.
    pub fn union(self, other: Rect<T, S>) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self::from_corners(
            self.origin.min(other.origin),
            self.far_corner().max(other.far_corner()),
        )
    }

    /// The rectangle shrunk by `edges`, never past zero extent.
    ///
    /// This is how a border box becomes a padding box and a padding box becomes a content box.
    pub fn inset(self, edges: Edges<T>) -> Self {
        let origin = Point::new(self.left() + edges.left, self.top() + edges.top);
        let size = Size::new(
            self.size.width - edges.horizontal(),
            self.size.height - edges.vertical(),
        );
        Self::new(origin, size.non_negative())
    }

    /// The rectangle grown by `edges`.
    pub fn outset(self, edges: Edges<T>) -> Self {
        let origin = Point::new(self.left() - edges.left, self.top() - edges.top);
        let size = Size::new(
            self.size.width + edges.horizontal(),
            self.size.height + edges.vertical(),
        );
        Self::new(origin, size)
    }

    /// The rectangle moved by `offset`.
    pub fn translate(self, offset: Size<T, S>) -> Self {
        Self::new(self.origin + offset, self.size)
    }

    /// The rectangle with every coordinate converted to another unit through [`f32`].
    pub fn to_unit<U: Unit>(self) -> Rect<U, S> {
        Rect::new(self.origin.to_unit(), self.size.to_unit())
    }
}

#[cfg(test)]
mod tests {
    use super::Rect;
    use crate::edges::Edges;
    use crate::point::Point;
    use crate::size::Size;
    use crate::space::Css;
    use crate::unit::CssPx;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<CssPx, Css> {
        Rect::new(
            Point::new(CssPx(x), CssPx(y)),
            Size::new(CssPx(width), CssPx(height)),
        )
    }

    #[test]
    fn containment_is_half_open() {
        let unit = rect(0.0, 0.0, 1.0, 1.0);
        assert!(unit.contains(Point::new(CssPx(0.0), CssPx(0.0))));
        assert!(!unit.contains(Point::new(CssPx(1.0), CssPx(0.0))));
        assert!(!unit.contains(Point::new(CssPx(0.0), CssPx(1.0))));
    }

    #[test]
    fn disjoint_rectangles_do_not_intersect() {
        assert_eq!(
            rect(0.0, 0.0, 1.0, 1.0).intersection(rect(2.0, 0.0, 1.0, 1.0)),
            None
        );
        assert_eq!(
            rect(0.0, 0.0, 1.0, 1.0).intersection(rect(1.0, 0.0, 1.0, 1.0)),
            None
        );
    }

    #[test]
    fn a_union_with_an_empty_rectangle_is_the_other_one() {
        let empty = rect(50.0, 50.0, 0.0, 0.0);
        let real = rect(0.0, 0.0, 10.0, 10.0);
        assert_eq!(real.union(empty), real);
        assert_eq!(empty.union(real), real);
    }

    #[test]
    fn insetting_past_zero_stops_at_zero() {
        let inset = rect(0.0, 0.0, 4.0, 4.0).inset(Edges::uniform(CssPx(10.0)));
        assert_eq!(inset.size, Size::new(CssPx(0.0), CssPx(0.0)));
        assert!(inset.is_empty());
    }

    #[test]
    fn corners_run_clockwise_from_the_top_left() {
        let corners = rect(1.0, 2.0, 3.0, 4.0).corners();
        assert_eq!(corners[0], Point::new(CssPx(1.0), CssPx(2.0)));
        assert_eq!(corners[1], Point::new(CssPx(4.0), CssPx(2.0)));
        assert_eq!(corners[2], Point::new(CssPx(4.0), CssPx(6.0)));
        assert_eq!(corners[3], Point::new(CssPx(1.0), CssPx(6.0)));
    }
}
