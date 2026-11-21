//! The pixels a filtered composite reads.

use zgui_geom::{Device, DevicePx, Edges, Rect};

use crate::fragment::filter::Filter;

/// Returns the pixels a composite of `bounds` under `filters` reads.
///
/// The chain applies in sequence, so the kernel supports of its filters add up. The result equals
/// `bounds` when every filter is per pixel.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_layout::fragment::filter::{Filter, read_extent};
///
/// let bounds: Rect<DevicePx, Device> = Rect::new(
///     Point::new(DevicePx(100.0), DevicePx(100.0)),
///     Size::new(DevicePx(50.0), DevicePx(50.0)),
/// );
///
/// assert_eq!(read_extent(bounds, &[Filter::Saturate(1.8)]), bounds);
///
/// let blurred = read_extent(bounds, &[Filter::Blur(4.0)]);
/// assert_eq!(blurred.origin, Point::new(DevicePx(86.0), DevicePx(86.0)));
/// assert_eq!(blurred.size, Size::new(DevicePx(78.0), DevicePx(78.0)));
/// ```
pub fn read_extent(bounds: Rect<DevicePx, Device>, filters: &[Filter]) -> Rect<DevicePx, Device> {
    let mut left = 0.0f32;
    let mut top = 0.0f32;
    let mut right = 0.0f32;
    let mut bottom = 0.0f32;
    for filter in filters {
        let (extra_left, extra_top, extra_right, extra_bottom) = filter.kernel_support();
        left += extra_left;
        top += extra_top;
        right += extra_right;
        bottom += extra_bottom;
    }

    if left == 0.0 && top == 0.0 && right == 0.0 && bottom == 0.0 {
        return bounds;
    }
    bounds.outset(Edges {
        top: DevicePx(top),
        right: DevicePx(right),
        bottom: DevicePx(bottom),
        left: DevicePx(left),
    })
}
