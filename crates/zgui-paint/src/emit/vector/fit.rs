//! Mapping the space a drawing was written in onto the box it is drawn in.
//!
//! An icon is written once, in a square of its own — twenty-four units on a side is the usual
//! choice — and drawn at a dozen sizes. A chart mark is written in the coordinates its own box was
//! placed with, because the same numbers decided where the box goes. Both are drawings and they
//! want opposite treatment, so the presence of a view box is what chooses between them.
//!
//! # Why fitting is uniform and centred
//!
//! A circle drawn in a square user space has to stay a circle in a box that is not square, so the
//! two axes take the same scale: the smaller of the two ratios, which is what makes the whole
//! drawing fit rather than the larger, which would crop it. What is left over is split evenly, so
//! a wide box centres its icon instead of pinning it to one edge. That is `xMidYMid meet` under
//! another name, and it is the only behaviour here because the alternatives are a property nothing
//! in this framework's vocabulary sets.

use zgui_geom::{Device, DevicePx, Rect};
use zgui_scene::kurbo::Affine;

/// The transform mapping a drawing's own coordinates onto the box it is drawn in.
///
/// `content_box` is in the fragment's local space, which is device pixels with no transform
/// applied, and the result is in that same space — so a drawing under a rotated ancestor is fitted
/// upright and rotated by the fragment's own matrix, exactly like every other primitive.
///
/// With a `view_box`, the drawing is scaled uniformly to fit and centred in what is left over.
/// Without one, the drawing is in CSS pixels measured from the content box's top left corner, so it
/// is scaled by the device pixel ratio and moved there.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_paint::emit::vector::fit::onto;
/// use zgui_scene::kurbo::Point as KurboPoint;
///
/// let box_ = Rect::new(
///     Point::new(DevicePx(10.0), DevicePx(20.0)),
///     Size::new(DevicePx(48.0), DevicePx(48.0)),
/// );
/// // A twenty-four unit square drawn into a forty-eight pixel box is drawn at twice the size.
/// let fitted = onto(box_, Some([0.0, 0.0, 24.0, 24.0]), 1.0);
/// assert_eq!(fitted * KurboPoint::new(24.0, 24.0), KurboPoint::new(58.0, 68.0));
/// assert_eq!(fitted * KurboPoint::new(0.0, 0.0), KurboPoint::new(10.0, 20.0));
/// ```
pub fn onto(content_box: Rect<DevicePx, Device>, view_box: Option<[f32; 4]>, scale: f32) -> Affine {
    let origin = (
        f64::from(content_box.origin.x.0),
        f64::from(content_box.origin.y.0),
    );
    let Some([x, y, width, height]) = view_box else {
        return Affine::translate(origin) * Affine::scale(f64::from(scale));
    };
    let (width, height) = (f64::from(width), f64::from(height));
    let available = (
        f64::from(content_box.size.width.0),
        f64::from(content_box.size.height.0),
    );
    let factor = (available.0 / width).min(available.1 / height);
    // Not `is_finite`: a box with no area gives a factor of zero, which collapses the drawing to a
    // point rather than producing a matrix full of infinities that a rasteriser has to reject.
    let factor = if factor.is_finite() {
        factor.max(0.0)
    } else {
        0.0
    };
    let slack = (
        (available.0 - width * factor) / 2.0,
        (available.1 - height * factor) / 2.0,
    );
    Affine::translate((origin.0 + slack.0, origin.1 + slack.1))
        * Affine::scale(factor)
        * Affine::translate((-f64::from(x), -f64::from(y)))
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_scene::kurbo::Point as KurboPoint;

    use super::onto;

    /// A box at the given origin and extent.
    fn box_(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    #[test]
    fn a_drawing_with_no_view_box_is_placed_at_its_box_in_css_pixels() {
        let fitted = onto(box_(10.0, 20.0, 100.0, 100.0), None, 2.0);
        assert_eq!(
            fitted * KurboPoint::new(0.0, 0.0),
            KurboPoint::new(10.0, 20.0)
        );
        assert_eq!(
            fitted * KurboPoint::new(8.0, 8.0),
            KurboPoint::new(26.0, 36.0),
            "eight CSS pixels is sixteen device pixels at twice the ratio"
        );
    }

    /// The whole point of a view box: one icon constant, drawn at whatever size its box is.
    #[test]
    fn a_view_box_scales_the_drawing_to_the_box_it_is_drawn_in() {
        let small = onto(
            box_(0.0, 0.0, 16.0, 16.0),
            Some([0.0, 0.0, 24.0, 24.0]),
            1.0,
        );
        let large = onto(
            box_(0.0, 0.0, 48.0, 48.0),
            Some([0.0, 0.0, 24.0, 24.0]),
            1.0,
        );
        assert_eq!(
            small * KurboPoint::new(24.0, 24.0),
            KurboPoint::new(16.0, 16.0)
        );
        assert_eq!(
            large * KurboPoint::new(24.0, 24.0),
            KurboPoint::new(48.0, 48.0)
        );
    }

    /// A round icon in an oblong box has to stay round, and sit in the middle of it.
    #[test]
    fn a_box_that_is_not_the_view_boxs_shape_fits_uniformly_and_centres_the_slack() {
        let fitted = onto(
            box_(0.0, 0.0, 100.0, 40.0),
            Some([0.0, 0.0, 20.0, 20.0]),
            1.0,
        );
        assert_eq!(
            fitted * KurboPoint::new(0.0, 0.0),
            KurboPoint::new(30.0, 0.0)
        );
        assert_eq!(
            fitted * KurboPoint::new(20.0, 20.0),
            KurboPoint::new(70.0, 40.0)
        );
    }

    #[test]
    fn a_view_box_with_an_offset_origin_puts_its_own_origin_at_the_boxs() {
        let fitted = onto(
            box_(5.0, 5.0, 10.0, 10.0),
            Some([-4.0, -4.0, 8.0, 8.0]),
            1.0,
        );
        assert_eq!(
            fitted * KurboPoint::new(-4.0, -4.0),
            KurboPoint::new(5.0, 5.0)
        );
        assert_eq!(
            fitted * KurboPoint::new(4.0, 4.0),
            KurboPoint::new(15.0, 15.0)
        );
    }

    /// A box collapsed to nothing must give a matrix a rasteriser can still consume.
    #[test]
    fn a_box_with_no_area_collapses_the_drawing_rather_than_producing_infinities() {
        let fitted = onto(box_(3.0, 4.0, 0.0, 0.0), Some([0.0, 0.0, 24.0, 24.0]), 1.0);
        let placed = fitted * KurboPoint::new(24.0, 24.0);
        assert!(placed.x.is_finite() && placed.y.is_finite());
        assert_eq!(placed, KurboPoint::new(3.0, 4.0));
    }
}
