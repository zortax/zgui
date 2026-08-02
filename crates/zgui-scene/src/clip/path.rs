//! A clip link as geometry, for anything that has to *draw* the clip rather than test against it.

use zgui_geom::{Corners, Device, DevicePx, Rect, Vec2};

use crate::clip::link::ClipLink;

/// How far along the tangent a cubic segment's control point sits when it approximates a quarter
/// ellipse.
///
/// The classic value: it makes the curve's midpoint land on the ellipse exactly and keeps the
/// largest radial error near one part in ten thousand of the radius, which is far under a device
/// pixel at any radius a document uses.
const KAPPA: f64 = 0.552_284_749_830_793_6;

/// The outline of a rounded rectangle, one elliptical arc per corner.
///
/// Every corner carries a *pair* of radii, because CSS says so: `border-radius: 80px / 20px` is an
/// ellipse quadrant, and a shape type with one scalar per corner cannot express it. A rasteriser
/// that fell back to a circular approximation would disagree with the same clip evaluated as a
/// distance field, in exactly the case the distance field goes to trouble to support.
///
/// Radii that overflow the box are shrunk by CSS's own uniform rule before any curve is emitted, so
/// adjacent corners meet rather than cross.
///
/// ```
/// use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
/// use zgui_scene::clip::path::rounded_rect;
///
/// let rect: Rect<DevicePx, Device> = Rect::new(
///     Point::new(DevicePx(0.0), DevicePx(0.0)),
///     Size::new(DevicePx(300.0), DevicePx(100.0)),
/// );
/// let radii = Corners::uniform(Vec2::new(DevicePx(80.0), DevicePx(20.0)));
/// let path = rounded_rect(rect, radii);
///
/// // The top edge leaves the straight run between the two corners' horizontal radii, and the
/// // curve reaches the side at the vertical one — which a single scalar per corner cannot say.
/// let bounds = kurbo::Shape::bounding_box(&path);
/// assert!((bounds.width() - 300.0).abs() < 0.01);
/// assert!((bounds.height() - 100.0).abs() < 0.01);
/// ```
pub fn rounded_rect(
    rect: Rect<DevicePx, Device>,
    radii: Corners<Vec2<DevicePx>>,
) -> kurbo::BezPath {
    let fitted = radii.fit_within(rect.size);
    let left = f64::from(rect.left().0);
    let top = f64::from(rect.top().0);
    let right = f64::from(rect.right().0);
    let bottom = f64::from(rect.bottom().0);
    let corner = |radius: Vec2<DevicePx>| {
        (
            f64::from(radius.x.0).max(0.0),
            f64::from(radius.y.0).max(0.0),
        )
    };
    let (tl_x, tl_y) = corner(fitted.top_left);
    let (tr_x, tr_y) = corner(fitted.top_right);
    let (br_x, br_y) = corner(fitted.bottom_right);
    let (bl_x, bl_y) = corner(fitted.bottom_left);
    let pull = 1.0 - KAPPA;

    let mut path = kurbo::BezPath::new();
    path.move_to((left + tl_x, top));
    path.line_to((right - tr_x, top));
    if tr_x > 0.0 || tr_y > 0.0 {
        path.curve_to(
            (right - tr_x * pull, top),
            (right, top + tr_y * pull),
            (right, top + tr_y),
        );
    }
    path.line_to((right, bottom - br_y));
    if br_x > 0.0 || br_y > 0.0 {
        path.curve_to(
            (right, bottom - br_y * pull),
            (right - br_x * pull, bottom),
            (right - br_x, bottom),
        );
    }
    path.line_to((left + bl_x, bottom));
    if bl_x > 0.0 || bl_y > 0.0 {
        path.curve_to(
            (left + bl_x * pull, bottom),
            (left, bottom - bl_y * pull),
            (left, bottom - bl_y),
        );
    }
    path.line_to((left, top + tl_y));
    if tl_x > 0.0 || tl_y > 0.0 {
        path.curve_to(
            (left, top + tl_y * pull),
            (left + tl_x * pull, top),
            (left + tl_x, top),
        );
    }
    path.close_path();
    path
}

/// The outline of one clip link, or `None` for a link that has no geometry to draw.
///
/// A sampled mask is the case with none: what survives of it in a display list is a coverage tile,
/// not the shape it was rasterised from, so nothing downstream can draw it as a path however the
/// shape began life.
pub fn of(link: &ClipLink) -> Option<kurbo::BezPath> {
    match link {
        ClipLink::RoundedRect { rect, radii, .. } => Some(rounded_rect(*rect, *radii)),
        ClipLink::Mask { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Shape as _;
    use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};

    use super::{of, rounded_rect};
    use crate::clip::link::{ClipLink, MaskSource};

    fn box_of(width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    #[test]
    fn a_square_cornered_rectangle_is_its_own_outline() {
        let path = rounded_rect(
            box_of(40.0, 20.0),
            Corners::uniform(Vec2::splat(DevicePx(0.0))),
        );
        assert_eq!(path.bounding_box(), kurbo::Rect::new(0.0, 0.0, 40.0, 20.0));
        assert!(
            (path.area().abs() - 800.0) < 0.001,
            "a square-cornered outline encloses the whole box"
        );
    }

    #[test]
    fn an_elliptical_corner_is_not_the_circular_one_with_either_radius() {
        // The whole reason a shape type carrying one scalar per corner is not enough: neither
        // circular reading has the area of the ellipse quadrant CSS asked for.
        let elliptical = rounded_rect(
            box_of(300.0, 100.0),
            Corners::uniform(Vec2::new(DevicePx(80.0), DevicePx(20.0))),
        )
        .area()
        .abs();
        let wide = rounded_rect(
            box_of(300.0, 100.0),
            Corners::uniform(Vec2::splat(DevicePx(80.0))),
        )
        .area()
        .abs();
        let narrow = rounded_rect(
            box_of(300.0, 100.0),
            Corners::uniform(Vec2::splat(DevicePx(20.0))),
        )
        .area()
        .abs();
        assert!(elliptical > wide + 1.0, "{elliptical} vs {wide}");
        assert!(elliptical < narrow - 1.0, "{elliptical} vs {narrow}");

        // Four quarter ellipses removed from the corners of the box leave exactly this much.
        // A cubic cannot be a quarter ellipse exactly; it can be within a part in twenty thousand
        // of one, which is what the control-point factor buys and what this pins.
        let expected = 300.0 * 100.0 - (4.0 - std::f64::consts::PI) * 80.0 * 20.0;
        assert!(
            (elliptical - expected).abs() / expected < 1.0e-4,
            "{elliptical} is not the area of a box with four elliptical corners ({expected})"
        );
    }

    #[test]
    fn radii_that_overflow_their_box_are_shrunk_rather_than_crossed() {
        let path = rounded_rect(
            box_of(100.0, 100.0),
            Corners::uniform(Vec2::splat(DevicePx(90.0))),
        );
        let bounds = path.bounding_box();
        assert!((bounds.width() - 100.0).abs() < 0.001);
        // Every radius collapses to half the side, so the outline is a circle of radius 50 and its
        // area is that of the circle rather than of anything larger.
        let area = path.area().abs();
        let circle = std::f64::consts::PI * 50.0 * 50.0;
        assert!(
            (area - circle).abs() / circle < 1.0e-3,
            "{area} vs {circle}"
        );
    }

    #[test]
    fn a_sampled_mask_has_no_outline_to_draw() {
        let tile = zgui_atlas::AtlasTile {
            texture: zgui_atlas::TextureId::new(zgui_atlas::TextureKind::Mono, 0),
            tile: zgui_atlas::TileId(0),
            bounds: Rect::new(Point::new(0, 0), Size::new(4, 4)),
        };
        for source in [MaskSource::Path, MaskSource::Raster] {
            assert!(
                of(&ClipLink::Mask {
                    tile,
                    transform: crate::spatial::SpatialId::VIEWPORT,
                    source,
                })
                .is_none(),
                "a coverage tile is not a path, whatever the shape it was rasterised from"
            );
        }
    }
}
