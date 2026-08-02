//! Turning a float rectangle into the whole-pixel region a pass covers.

use zgui_geom::{Device, DevicePx, Point, Rect, Size};

/// How a pass region's edges are aligned.
///
/// Rasterisers work in tiles, and a region whose origin is not on the tile grid makes every tile
/// straddle two of them. Sixteen is the tile size the technique is built around; aligning outwards
/// can only make a region larger, so nothing is ever cut off.
pub const TILE: i32 = 16;

/// The smallest whole-pixel rectangle containing `rect`.
///
/// Outward rounding, so a shape that covers a fraction of an edge pixel still gets that pixel.
pub fn covering(rect: Rect<DevicePx, Device>) -> Rect<i32, Device> {
    let left = rect.origin.x.0.floor() as i32;
    let top = rect.origin.y.0.floor() as i32;
    let right = (rect.origin.x.0 + rect.size.width.0).ceil() as i32;
    let bottom = (rect.origin.y.0 + rect.size.height.0).ceil() as i32;
    Rect::new(
        Point::new(left, top),
        Size::new((right - left).max(0), (bottom - top).max(0)),
    )
}

/// The tile-aligned region covering `rect`, clamped to a surface of `viewport`.
///
/// Empty when the rectangle lies wholly outside the surface, which is the answer a caller wants:
/// there is nothing there to rasterise.
pub fn aligned(rect: Rect<DevicePx, Device>, viewport: Size<i32, Device>) -> Rect<i32, Device> {
    let covered = covering(rect);
    let left = align_down(covered.origin.x).clamp(0, viewport.width);
    let top = align_down(covered.origin.y).clamp(0, viewport.height);
    let right = align_up(covered.origin.x + covered.size.width).clamp(0, viewport.width);
    let bottom = align_up(covered.origin.y + covered.size.height).clamp(0, viewport.height);
    Rect::new(
        Point::new(left, top),
        Size::new((right - left).max(0), (bottom - top).max(0)),
    )
}

/// `value` rounded down to a multiple of [`TILE`].
fn align_down(value: i32) -> i32 {
    value.div_euclid(TILE) * TILE
}

/// `value` rounded up to a multiple of [`TILE`].
fn align_up(value: i32) -> i32 {
    align_down(value + TILE - 1)
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use super::{TILE, aligned, covering};

    /// A device rectangle.
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    #[test]
    fn covering_rounds_outwards_on_every_edge() {
        let covered = covering(rect(1.2, 2.7, 3.1, 4.1));
        assert_eq!(covered.origin, Point::new(1, 2));
        assert_eq!(covered.size, Size::new(4, 5));
    }

    #[test]
    fn alignment_grows_to_the_tile_grid_and_never_shrinks() {
        let region = aligned(rect(20.0, 20.0, 10.0, 10.0), Size::new(1920, 1080));
        assert_eq!(region.origin, Point::new(16, 16));
        assert_eq!(region.size, Size::new(TILE, TILE));
        assert!(region.contains_rect(covering(rect(20.0, 20.0, 10.0, 10.0))));
    }

    #[test]
    fn a_region_outside_the_surface_is_empty() {
        let region = aligned(rect(4000.0, 20.0, 10.0, 10.0), Size::new(1920, 1080));
        assert!(region.is_empty());
    }

    #[test]
    fn a_region_is_clamped_to_the_surface_rather_than_hanging_off_it() {
        let region = aligned(rect(1900.0, 1070.0, 100.0, 100.0), Size::new(1920, 1080));
        assert_eq!(region.right(), 1920);
        assert_eq!(region.bottom(), 1080);
    }
}
