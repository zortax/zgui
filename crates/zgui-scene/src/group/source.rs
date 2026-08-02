//! The one function that says which pixels a filtered composite reads.

use zgui_geom::{Device, DevicePx, Edges, Rect};

use crate::group::filter::Filter;

/// Every pixel a composite of `bounds` under `filters` **reads**, which is `bounds` inflated by the
/// chain's kernel support.
///
/// It is a pure function of a rectangle and a filter chain, and it has more than one caller on
/// purpose. A group boundary stores it, a backdrop filter stores it, the damage expansion that runs
/// before anything is emitted evaluates it, and the rule that decides whether a composite survives
/// culling consults it. Those four agreeing is not a coincidence to be maintained — they call this.
///
/// Degenerate — equal to `bounds` — for every per-pixel filter, for every blend mode and for plain
/// opacity, which is nearly every group there is. A caller can therefore test `source == bounds` to
/// find the small set of composites that read outside themselves.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_scene::{Filter, read_extent};
///
/// let bounds: Rect<DevicePx, Device> = Rect::new(
///     Point::new(DevicePx(100.0), DevicePx(100.0)),
///     Size::new(DevicePx(50.0), DevicePx(50.0)),
/// );
///
/// // A per-pixel filter reads exactly what it writes.
/// assert_eq!(read_extent(bounds, &[Filter::Saturate(1.8)]), bounds);
///
/// // A blur reads three standard deviations beyond it, on every side.
/// let blurred = read_extent(bounds, &[Filter::Blur(4.0)]);
/// assert_eq!(blurred.origin, Point::new(DevicePx(88.0), DevicePx(88.0)));
/// assert_eq!(blurred.size, Size::new(DevicePx(74.0), DevicePx(74.0)));
/// ```
pub fn read_extent(bounds: Rect<DevicePx, Device>, filters: &[Filter]) -> Rect<DevicePx, Device> {
    let mut left = 0.0f32;
    let mut top = 0.0f32;
    let mut right = 0.0f32;
    let mut bottom = 0.0f32;
    for filter in filters {
        // The chain applies in sequence, so each filter reads outside what the one before it
        // already reached: the supports add rather than taking a maximum.
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
