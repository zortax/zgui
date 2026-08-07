//! The seam a custom element's paint half plugs into, and the constrained painter it draws with.
//!
//! A custom element's primitives land at Appendix E step 4 — inside its own background and
//! border, before its descendants, under its fragment's clip and transform — which is the same
//! argument the vector arm makes: sorted, clipped, moved and faded exactly like a background.
//! What is different is who produces them, and [`ScenePainter`] is the door that keeps that safe:
//! every push goes through the scene's own insertion path, so draw-order assignment, clip culling
//! and replay accounting hold whatever the implementation does.

use std::sync::Arc;

use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_scene::kurbo;
use zgui_scene::prim::quad::BorderStyle;
use zgui_scene::{ClipId, PaintRef, Quad, Scene, SpatialId, VectorId};

/// Where a custom element's painting comes from.
///
/// The paint stage asks two questions and nothing else: *has it changed* — the revision, which is
/// what lets an untouched element replay its recorded primitives — and *what does it draw*, asked
/// only on the frames where the answer cannot be replayed.
pub trait CustomPaintSource {
    /// A monotone revision of what the element `token` names paints; part of the fragment's
    /// replay record. Asked of the registry rather than of anything the frame captured, because
    /// a repaint moves nothing but this number.
    fn revision(&self, token: u32) -> u64;

    /// Emits the element's own primitives through the painter.
    fn paint(&self, token: u32, painter: &mut ScenePainter<'_>);
}

/// A source with no custom elements in it.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoCustom;

impl CustomPaintSource for NoCustom {
    fn revision(&self, _token: u32) -> u64 {
        0
    }

    fn paint(&self, _token: u32, _painter: &mut ScenePainter<'_>) {}
}

/// One frame's painting surface for one custom element.
///
/// Coordinates are device pixels measured from the element's **content box** corner; the painter
/// translates. The clip, the transform and any folded group alpha are the fragment's own and are
/// applied to everything — an implementation cannot escape its box's clipping any more than a
/// background can.
///
/// What it exposes is deliberately the invariant-preserving subset: solid-filled and stroked
/// rounded rectangles through the quad pipeline, and arbitrary paths through the vector pipeline
/// with per-shape paint. No group or layer boundaries — those are matched pairs the walk manages
/// — and no clip creation: an element wanting an inner clip gives a child `overflow: hidden`.
pub struct ScenePainter<'a> {
    /// The display list being built.
    pub(crate) scene: &'a mut Scene,
    /// The element's content box, whose corner is the painter's origin.
    pub(crate) content_box: Rect<DevicePx, Device>,
    /// The fragment's clip chain.
    pub(crate) clip: ClipId,
    /// The fragment's coordinate system.
    pub(crate) transform: SpatialId,
    /// The alpha folded in from groups above.
    pub(crate) alpha: f32,
    /// Device pixels per CSS pixel.
    pub(crate) scale: f32,
    /// The element's computed `color`, resolved for inherited brushes.
    pub(crate) shape_paint: crate::emit::vector::ShapePaint,
    /// The shared cache used by eligible solid paths.
    pub(crate) vector_masks: &'a dyn crate::content::vectors::VectorMaskSource,
    /// The identity vector items are encoded under, from the fragment.
    pub(crate) vector_id: VectorId,
    /// How many shapes have been pushed, so each gets a distinct sub-identity.
    pub(crate) shapes_pushed: u32,
    /// How many primitives went in, reported to the walk.
    pub(crate) pushed: usize,
}

impl ScenePainter<'_> {
    /// The element's content box size, in device pixels.
    pub fn size(&self) -> Size<DevicePx, Device> {
        self.content_box.size
    }

    /// Device pixels per CSS pixel.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The element's computed `color`, for painting content that follows the text around it.
    pub fn current_color(&self) -> Color {
        self.shape_paint.fill
    }

    /// Fills a rounded rectangle with one colour, through the quad pipeline.
    ///
    /// This is the cheap path — the one a background costs — and the reason a custom element
    /// exists at all: retained widgets built from it pay neither a texture nor a rasterisation.
    pub fn fill(&mut self, rect: Rect<DevicePx, Device>, corner_radius: f32, color: Color) {
        let quad = Quad::filled(
            self.placed(rect),
            PaintRef::solid(self.scene.paints.solid(self.faded(color))),
        )
        .with_radii(corners(corner_radius))
        .clipped(self.clip);
        self.push_quad(quad);
    }

    /// Strokes a rounded rectangle `width` wide with one colour, through the quad pipeline.
    pub fn stroke(
        &mut self,
        rect: Rect<DevicePx, Device>,
        corner_radius: f32,
        width: f32,
        color: Color,
    ) {
        let stroke = PaintRef::solid(self.scene.paints.solid(self.faded(color)));
        let quad = Quad::filled(self.placed(rect), PaintRef::NONE)
            .with_radii(corners(corner_radius))
            .with_border([width; 4], stroke, BorderStyle::Solid)
            .clipped(self.clip);
        self.push_quad(quad);
    }

    /// Draws one shape — a path with its own fill and stroke — through the vector pipeline.
    ///
    /// The path is in the painter's coordinates; per-shape paint is the whole shape vocabulary a
    /// vector document has, including [`Ink::Inherited`](zgui_svg::Ink) resolving to the
    /// element's `color`. Dearer than [`fill`](ScenePainter::fill) — a shape is rasterised — so a
    /// widget reaches for it for the geometry quads cannot say.
    pub fn shape(&mut self, shape: &zgui_svg::Shape) {
        let placed = zgui_svg::document::place::shape(
            shape,
            kurbo::Affine::translate((
                f64::from(self.content_box.origin.x.0),
                f64::from(self.content_box.origin.y.0),
            )),
        );
        self.shapes_pushed += 1;
        // The fragment's identity in the high bits, the shape's index in the low: stable across
        // frames for the same fragment, distinct within it, which is what the rasteriser's
        // encoding cache keys on.
        let id = VectorId((self.vector_id.0 << 16) | (self.shapes_pushed & 0xFFFF));
        self.pushed += crate::emit::vector::document::emit(
            self.scene,
            id,
            &placed,
            &self.shape_paint,
            self.vector_masks,
            crate::emit::vector::VectorPlacement {
                clip: self.clip,
                transform: self.transform,
                scale: self.scale,
            },
        );
    }

    /// A path convenience over [`ScenePainter::shape`]: fills `path` with one colour.
    pub fn fill_path(&mut self, path: impl Into<kurbo::BezPath>, color: Color) {
        self.shape(&zgui_svg::Shape {
            path: Arc::new(path.into()),
            fill: Some(zgui_svg::Fill {
                paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Solid(color)),
                rule: zgui_scene::peniko::Fill::NonZero,
            }),
            stroke: None,
            clips: Vec::new(),
        });
    }

    /// The rectangle, moved from painter coordinates onto the device.
    fn placed(&self, rect: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(
                DevicePx(self.content_box.origin.x.0 + rect.origin.x.0),
                DevicePx(self.content_box.origin.y.0 + rect.origin.y.0),
            ),
            rect.size,
        )
    }

    /// The colour at the fragment's folded alpha.
    fn faded(&self, color: Color) -> Color {
        color.with_alpha(color.alpha() * self.alpha)
    }

    /// Pushes one quad under the fragment's transform, counting what survived the cull.
    fn push_quad(&mut self, quad: Quad) {
        let quad = quad.transformed(self.transform);
        self.pushed += usize::from(self.scene.push_quad(quad).is_some());
    }
}

/// Uniform corner radii in the shape quads carry them.
fn corners(radius: f32) -> Corners<Vec2<DevicePx>> {
    let corner = Vec2::new(DevicePx(radius), DevicePx(radius));
    Corners {
        top_left: corner,
        top_right: corner,
        bottom_left: corner,
        bottom_right: corner,
    }
}
