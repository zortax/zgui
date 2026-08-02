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
