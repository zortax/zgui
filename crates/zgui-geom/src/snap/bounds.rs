// DERIVED-FROM: the GPUI project, crates/gpui/src/window.rs (Apache-2.0)
// The two rectangle snapping rules here — rounding every edge to the nearest device pixel, and
// flooring the near edges while ceiling the far ones so the result covers the input — are adapted
// from that work, licensed under the Apache License, Version 2.0, and have been modified to work
// over this crate's coordinate spaces and units.

//! Rectangle snapping.

use crate::point::Point;
use crate::rect::Rect;
use crate::snap::{ceil_length, floor_length, snap_length};
use crate::space::{Css, Device};
use crate::unit::{CssPx, DevicePx, Scale};

/// Scales a rectangle into device space and rounds every edge to the nearest device pixel.
///
/// This is the rule for geometry that is drawn: a background, a border box, a glyph run. Edges
/// land on pixel boundaries, so a border is the width it claims to be instead of a blur across
/// two columns of pixels.
///
/// The far edges never move past the near ones, so a rectangle that was not inverted does not
/// become inverted; it can collapse to zero extent, which is the honest answer for something
/// smaller than half a pixel.
///
/// ```
/// use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Rect, Scale, Size, snap_bounds};
///
/// let scale: Scale<Css, Device> = Scale::new(1.5);
/// let rect: Rect<CssPx, Css> = Rect::new(
///     Point::new(CssPx(1.0), CssPx(1.0)),
///     Size::new(CssPx(10.0), CssPx(10.0)),
/// );
/// let snapped = snap_bounds(rect, scale);
/// // 1.5 device pixels is an exact tie, which breaks toward zero; 16.5 rounds down likewise.
/// assert_eq!(snapped.origin, Point::new(DevicePx(1.0), DevicePx(1.0)));
/// assert_eq!(snapped.far_corner(), Point::new(DevicePx(16.0), DevicePx(16.0)));
/// ```
pub fn snap_bounds(rect: Rect<CssPx, Css>, scale: Scale<Css, Device>) -> Rect<DevicePx, Device> {
    snap_device_bounds(rect * scale)
}

/// Rounds every edge of a device-space rectangle to the nearest device pixel.
///
/// [`snap_bounds`] is this rule preceded by the scale; use this one where the geometry has already
/// been converted. Applying it twice changes nothing.
pub fn snap_device_bounds(rect: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
    let near = rect.origin.map(snap_length);
    let far = rect.far_corner().map(snap_length).max(near);
    Rect::new(near, far.offset_from(near))
}

/// Scales a rectangle into device space and grows it to the smallest covering pixel rectangle.
///
/// This is the rule for geometry that bounds other geometry: a clip rectangle, a damage region, a
/// scissor rectangle. The result contains every point the input contained, so nothing that should
/// have been drawn is clipped away by a rounding decision.
///
/// ```
/// use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Rect, Scale, Size, cover_bounds};
///
/// let scale: Scale<Css, Device> = Scale::new(1.0);
/// let rect: Rect<CssPx, Css> = Rect::new(
///     Point::new(CssPx(0.25), CssPx(0.25)),
///     Size::new(CssPx(1.5), CssPx(1.5)),
/// );
/// let covered = cover_bounds(rect, scale);
/// assert_eq!(covered.origin, Point::new(DevicePx(0.0), DevicePx(0.0)));
/// assert_eq!(covered.far_corner(), Point::new(DevicePx(2.0), DevicePx(2.0)));
/// ```
pub fn cover_bounds(rect: Rect<CssPx, Css>, scale: Scale<Css, Device>) -> Rect<DevicePx, Device> {
    cover_device_bounds(rect * scale)
}

/// Grows a device-space rectangle to the smallest rectangle on the pixel grid that contains it.
///
/// [`cover_bounds`] is this rule preceded by the scale. Applying it twice changes nothing.
pub fn cover_device_bounds(rect: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
    let near: Point<DevicePx, Device> = rect.origin.map(floor_length);
    let far = rect.far_corner().map(ceil_length).max(near);
    Rect::new(near, far.offset_from(near))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{cover_bounds, cover_device_bounds, snap_bounds, snap_device_bounds};
    use crate::point::Point;
    use crate::rect::Rect;
    use crate::size::Size;
    use crate::space::{Css, Device};
    use crate::unit::{CssPx, DevicePx, Scale};

    /// A well-formed CSS-space rectangle, plus a plausible device pixel ratio.
    fn case() -> impl Strategy<Value = (Rect<CssPx, Css>, Scale<Css, Device>)> {
        (
            -4000.0_f32..4000.0,
            -4000.0_f32..4000.0,
            0.0_f32..4000.0,
            0.0_f32..4000.0,
            0.25_f32..4.0,
        )
            .prop_map(|(x, y, width, height, factor)| {
                (
                    Rect::new(
                        Point::new(CssPx(x), CssPx(y)),
                        Size::new(CssPx(width), CssPx(height)),
                    ),
                    Scale::new(factor),
                )
            })
    }

    #[test]
    fn a_sub_pixel_rectangle_may_collapse_but_never_inverts() {
        let scale: Scale<Css, Device> = Scale::new(1.0);
        let sliver: Rect<CssPx, Css> = Rect::new(
            Point::new(CssPx(10.0), CssPx(10.0)),
            Size::new(CssPx(0.2), CssPx(0.2)),
        );
        let snapped = snap_bounds(sliver, scale);
        assert_eq!(snapped.size, Size::new(DevicePx(0.0), DevicePx(0.0)));
        assert!(snapped.right() >= snapped.left());
        assert!(snapped.bottom() >= snapped.top());
    }

    #[test]
    fn covering_keeps_a_sub_pixel_rectangle_visible() {
        let scale: Scale<Css, Device> = Scale::new(1.0);
        let sliver: Rect<CssPx, Css> = Rect::new(
            Point::new(CssPx(10.1), CssPx(10.1)),
            Size::new(CssPx(0.2), CssPx(0.2)),
        );
        assert_eq!(
            cover_bounds(sliver, scale).size,
            Size::new(DevicePx(1.0), DevicePx(1.0))
        );
    }

    proptest! {
        /// Snapping an already-snapped rectangle changes nothing.
        #[test]
        fn snapping_is_idempotent((rect, scale) in case()) {
            let once = snap_bounds(rect, scale);
            prop_assert_eq!(snap_device_bounds(once), once);
            prop_assert_eq!(snap_device_bounds(snap_device_bounds(once)), once);
        }

        /// Covering an already-covered rectangle changes nothing.
        #[test]
        fn covering_is_idempotent((rect, scale) in case()) {
            let once = cover_bounds(rect, scale);
            prop_assert_eq!(cover_device_bounds(once), once);
        }

        /// A snapped rectangle sits exactly on the device pixel grid.
        #[test]
        fn snapped_edges_land_on_the_grid((rect, scale) in case()) {
            let snapped = snap_bounds(rect, scale);
            prop_assert!(snapped.left().is_grid_aligned());
            prop_assert!(snapped.top().is_grid_aligned());
            prop_assert!(snapped.right().is_grid_aligned());
            prop_assert!(snapped.bottom().is_grid_aligned());
        }

        /// The covering rectangle contains the rectangle it was computed from.
        #[test]
        fn covering_contains_the_original((rect, scale) in case()) {
            let scaled = rect * scale;
            let covered = cover_bounds(rect, scale);
            prop_assert!(covered.left() <= scaled.left());
            prop_assert!(covered.top() <= scaled.top());
            prop_assert!(covered.right() >= scaled.right());
            prop_assert!(covered.bottom() >= scaled.bottom());
            prop_assert!(covered.contains_rect(scaled), "{:?} does not contain {:?}", covered, scaled);
        }

        /// Snapping moves no edge by more than half a device pixel.
        #[test]
        fn snapping_moves_nothing_far((rect, scale) in case()) {
            let scaled = rect * scale;
            let snapped = snap_bounds(rect, scale);
            prop_assert!((snapped.left() - scaled.left()).abs() <= DevicePx(0.5));
            prop_assert!((snapped.top() - scaled.top()).abs() <= DevicePx(0.5));
        }
    }
}
