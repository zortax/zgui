//! Vector content: shapes a path rasteriser draws and this crate composites back in.

pub mod clip;
pub mod stroke;

use std::sync::Arc;

use zgui_geom::{Device, DevicePx, Rect};

use crate::id::{ClipId, DrawOrder, VectorId};
use crate::paint::PaintRef;
use crate::spatial::SpatialId;
use crate::vector::clip::VectorClip;
use crate::vector::stroke::VectorStroke;

/// One filled or stroked path.
///
/// The geometry is `kurbo`'s and the fill rule is `peniko`'s — pure-data vocabularies with no GPU
/// in them — so a rasteriser consumes exactly what is stored here with no conversion, while nothing
/// about this type names any particular rasteriser.
///
/// The path is shared rather than owned, because the same geometry is re-placed every frame and a
/// rasteriser keeps its own encoded form of it under [`VectorItem::id`].
#[derive(Clone, Debug)]
pub struct VectorItem {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// Stable identity across frames, so a rasteriser can cache its encoding of the geometry.
    pub id: VectorId,
    /// The geometry, in device space.
    pub path: Arc<kurbo::BezPath>,
    /// Everything this paints, on the device — the item's transform applied. Derived from the path
    /// and the stroke width, and stored because the rasterisation pass and the composite both read
    /// it and neither should re-measure a path.
    pub ink: Rect<DevicePx, Device>,
    /// The same reach, measured in the subtree's own space — the transform *not* applied.
    ///
    /// What the draw order and the clip cull read, and it has to be this one: every other
    /// primitive's ink is recorded before any transform, so an order taken over the device-space
    /// rectangle instead puts a drawing above or below neighbours it never overlaps in the space
    /// the order is decided in. A dialog holding the placement its entrance settled on showed
    /// exactly that — its icon ordered against nothing, painted first, and covered by the surface
    /// drawn over it.
    pub local_ink: Rect<DevicePx, Device>,
    /// What fills the path, if anything.
    pub fill: Option<PaintRef>,
    /// How a filled path decides what is inside it.
    pub fill_rule: peniko::Fill,
    /// What strokes the path and in what shape, if anything.
    pub stroke: Option<VectorStroke>,
    /// The chain this draws through.
    pub clip: ClipId,
    /// Outlines this is kept inside, which apply together and inside the chain.
    ///
    /// Empty for everything a stylesheet produces: a CSS clip is a rectangle with corners and
    /// travels in [`VectorItem::clip`], where every primitive can be tested against it. This is
    /// what a vector document's own `clipPath` becomes, which no shader evaluates.
    pub clips: Vec<VectorClip>,
    /// The transform this draws under.
    pub transform: Option<SpatialId>,
}

impl VectorItem {
    /// A non-zero filled path.
    ///
    /// The ink is measured from the path's control-point bounding box, which contains the curve, so
    /// it can only over-report and never leave a stale pixel behind.
    pub fn filled(id: VectorId, path: Arc<kurbo::BezPath>, fill: PaintRef) -> Self {
        let ink = ink_of(&path, 0.0);
        Self {
            order: 0,
            id,
            path,
            ink,
            local_ink: ink,
            fill: Some(fill),
            fill_rule: peniko::Fill::NonZero,
            stroke: None,
            clip: ClipId::ROOT,
            clips: Vec::new(),
            transform: None,
        }
    }

    /// A stroked path of the given width.
    pub fn stroked(id: VectorId, path: Arc<kurbo::BezPath>, stroke: PaintRef, width: f32) -> Self {
        Self::styled(id, path, VectorStroke::solid(stroke, width))
    }

    /// A path stroked in the given style.
    ///
    /// The ink accounts for the whole style rather than for the width alone, because a mitred
    /// corner and a square cap both put ink outside the half-width band a plain stroke covers.
    pub fn styled(id: VectorId, path: Arc<kurbo::BezPath>, stroke: VectorStroke) -> Self {
        let ink = ink_of(&path, stroke.reach() * 2.0);
        Self {
            order: 0,
            id,
            path,
            ink,
            local_ink: ink,
            fill: None,
            fill_rule: peniko::Fill::NonZero,
            stroke: Some(stroke),
            clip: ClipId::ROOT,
            clips: Vec::new(),
            transform: None,
        }
    }

    /// The same item drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip;
        self
    }

    /// The same item kept inside `clips` as well as inside its chain.
    pub fn inside(mut self, clips: Vec<VectorClip>) -> Self {
        self.clips = clips;
        self
    }

    /// The same item with the even-odd fill rule.
    pub fn even_odd(mut self) -> Self {
        self.fill_rule = peniko::Fill::EvenOdd;
        self
    }
}

/// The rectangle a path of the given stroke width can put ink in.
fn ink_of(path: &kurbo::BezPath, stroke_width: f32) -> Rect<DevicePx, Device> {
    let box2 = path.control_box();
    let half = f64::from(stroke_width) / 2.0;
    Rect::new(
        zgui_geom::Point::new(
            DevicePx((box2.x0 - half) as f32),
            DevicePx((box2.y0 - half) as f32),
        ),
        zgui_geom::Size::new(
            DevicePx((box2.width() + 2.0 * half) as f32),
            DevicePx((box2.height() + 2.0 * half) as f32),
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kurbo::Shape;

    use zgui_geom::DevicePx;

    use crate::id::VectorId;
    use crate::paint::PaintRef;
    use crate::vector::VectorItem;

    /// A square path.
    fn square() -> Arc<kurbo::BezPath> {
        Arc::new(kurbo::Rect::new(10.0, 20.0, 30.0, 60.0).into_path(0.1))
    }

    #[test]
    fn a_filled_paths_ink_is_its_bounding_box() {
        let item = VectorItem::filled(VectorId(0), square(), PaintRef::NONE);
        assert_eq!(item.ink.origin.x, DevicePx(10.0));
        assert_eq!(item.ink.size.width, DevicePx(20.0));
        assert_eq!(item.ink.size.height, DevicePx(40.0));
    }

    #[test]
    fn a_stroke_widens_the_ink_by_half_its_width_on_every_side() {
        let item = VectorItem::stroked(VectorId(0), square(), PaintRef::NONE, 4.0);
        assert_eq!(item.ink.origin.x, DevicePx(8.0));
        assert_eq!(item.ink.size.width, DevicePx(24.0));
    }

    /// A mitred corner reaches past the half width, and the damage rectangle has to contain it or
    /// the tip of the corner stays on the screen after the shape has gone.
    #[test]
    fn a_mitred_stroke_widens_the_ink_past_half_its_width() {
        let stroke = crate::vector::stroke::VectorStroke {
            paint: PaintRef::NONE,
            style: kurbo::Stroke::new(4.0)
                .with_join(kurbo::Join::Miter)
                .with_miter_limit(4.0),
        };
        let item = VectorItem::styled(VectorId(0), square(), stroke);
        assert_eq!(item.ink.origin.x, DevicePx(2.0));
        assert_eq!(item.ink.size.width, DevicePx(36.0));
    }

    #[test]
    fn an_item_carries_no_shape_clips_unless_it_is_given_some() {
        let item = VectorItem::filled(VectorId(0), square(), PaintRef::NONE);
        assert!(item.clips.is_empty());
        let clipped = item.inside(vec![crate::vector::clip::VectorClip::new(square())]);
        assert_eq!(clipped.clips.len(), 1);
    }
}
