//! The axis-aligned box a transformed rectangle occupies.

use crate::rect::Rect;
use crate::space::Device;
use crate::transform::Matrix4;
use crate::unit::DevicePx;

/// The axis-aligned rectangle that contains `rect` after `matrix` is applied to it.
///
/// A transformed rectangle is not a rectangle, so what damage, culling and a spatial index all
/// need is the smallest axis-aligned box containing its four corners. Points behind the viewer
/// under a perspective matrix are dropped rather than projected, because their projection is a
/// reflection through the origin and would report ink on the wrong side of the screen; a rectangle
/// with no corner in front of the viewer occupies nothing at all.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size, transformed_bounds};
///
/// let rect: Rect<DevicePx, Device> = Rect::new(
///     Point::new(DevicePx(0.0), DevicePx(0.0)),
///     Size::new(DevicePx(10.0), DevicePx(4.0)),
/// );
/// let moved = transformed_bounds(&Matrix4::translation(3.0, 5.0, 0.0), rect);
/// assert_eq!(moved.origin.x, DevicePx(3.0));
/// assert_eq!(moved.size.width, DevicePx(10.0));
/// ```
pub fn transformed_bounds(
    matrix: &Matrix4,
    rect: Rect<DevicePx, Device>,
) -> Rect<DevicePx, Device> {
    // A translation in the plane moves each corner by the same amount. The sums are the ones the
    // general path below makes, so both paths give the same rectangle.
    if let Some((x, y)) = plane_translation(matrix) {
        let (left, right) = (rect.left().0 + x, rect.right().0 + x);
        let (top, bottom) = (rect.top().0 + y, rect.bottom().0 + y);
        return Rect::from_corners(
            crate::point::Point::new(DevicePx(left.min(right)), DevicePx(top.min(bottom))),
            crate::point::Point::new(DevicePx(left.max(right)), DevicePx(top.max(bottom))),
        );
    }
    let corners = [
        (rect.left().0, rect.top().0),
        (rect.right().0, rect.top().0),
        (rect.right().0, rect.bottom().0),
        (rect.left().0, rect.bottom().0),
    ];
    let mut min = (f32::INFINITY, f32::INFINITY);
    let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut seen = 0;
    for (x, y) in corners {
        let projected = matrix.transform_vector4([x, y, 0.0, 1.0]);
        if projected[3] <= 0.0 {
            continue;
        }
        let inverse_w = projected[3].recip();
        let (px, py) = (projected[0] * inverse_w, projected[1] * inverse_w);
        min = (min.0.min(px), min.1.min(py));
        max = (max.0.max(px), max.1.max(py));
        seen += 1;
    }
    if seen == 0 {
        return Rect::ZERO;
    }
    Rect::from_corners(
        crate::point::Point::new(DevicePx(min.0), DevicePx(min.1)),
        crate::point::Point::new(DevicePx(max.0), DevicePx(max.1)),
    )
}

/// How far `matrix` moves a point in the plane, when that is all it does to one.
///
/// The third column is left out: it multiplies the depth of a point in the plane, which is zero.
fn plane_translation(matrix: &Matrix4) -> Option<(f32, f32)> {
    let [x, y, _, w] = matrix.columns;
    let moves_only = x[0] == 1.0
        && x[1] == 0.0
        && x[3] == 0.0
        && y[0] == 0.0
        && y[1] == 1.0
        && y[3] == 0.0
        && w[3] == 1.0;
    moves_only.then_some((w[0], w[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Point, Size};

    #[test]
    fn a_translation_gives_the_rectangle_its_corners_give() {
        let rect: Rect<DevicePx, Device> = Rect::new(
            Point::new(DevicePx(-3.25), DevicePx(17.1)),
            Size::new(DevicePx(120.7), DevicePx(0.3)),
        );
        for (x, y) in [(0.0, -1234.56), (0.1, 0.2), (-4096.0, -4096.0), (7.5, 3e6)] {
            let matrix = Matrix4::translation(x, y, 0.0);
            assert!(plane_translation(&matrix).is_some());
            let corners = [
                matrix.transform_point(rect.left().0, rect.top().0, 0.0),
                matrix.transform_point(rect.right().0, rect.bottom().0, 0.0),
            ];
            let expected = Rect::from_corners(
                Point::new(DevicePx(corners[0][0]), DevicePx(corners[0][1])),
                Point::new(DevicePx(corners[1][0]), DevicePx(corners[1][1])),
            );
            assert_eq!(transformed_bounds(&matrix, rect), expected);
        }
        assert!(plane_translation(&Matrix4::scale(2.0, 1.0, 1.0)).is_none());
        let mut tilted = Matrix4::IDENTITY;
        tilted.columns[0][3] = 0.01;
        assert!(plane_translation(&tilted).is_none());
    }
}
