//! The seam a custom element's paint half plugs into, and the constrained painter it draws with.
//!
//! A custom element's items land at Appendix E step 4, inside its own background and border,
//! before its descendants, under its fragment's clip and transform. That is the same argument the
//! vector arm makes: they are ordered, clipped, moved and faded exactly like a background. What is
//! different is who produces them, and [`ScenePainter`] is the door that keeps that safe: every
//! item goes through the fragment's recorder, under the fragment's clip and transform.

use std::sync::Arc;

use compositor_types::{BorderStyle, PaintRef, Quad};
use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_scene::kurbo;
use zgui_scene::{ClipId, ShadedQuad, ShaderId, ShaderParams, SpatialId, VectorId};

use crate::emit::record::{Recorder, flat_radii, local_rect};

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
/// rounded rectangles through the quad pipeline, arbitrary paths through the vector pipeline with
/// per-shape paint, and runs of glyphs. No group or layer boundaries — those are matched pairs the
/// walk manages — and no clip creation: an element wanting an inner clip gives a child
/// `overflow: hidden`.
pub struct ScenePainter<'a> {
    /// The fragment's chunk, and the display list behind it.
    pub(crate) rec: Recorder<'a>,
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
    /// Whether per-channel coverage survives this fragment's destination and transform.
    pub(crate) text_subpixel: bool,
    /// The identity vector items are encoded under, from the fragment.
    pub(crate) vector_id: VectorId,
    /// How many shapes have been pushed, so each gets a distinct sub-identity.
    pub(crate) shapes_pushed: u32,
    /// Every vector raster path selected by this custom element's shapes.
    pub(crate) vector_routes: crate::emit::vector::VectorRoutes,
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

    /// Where the element's content box sits on the surface, in device pixels.
    ///
    /// Everything a painter draws is measured from this corner, so an element never needs it to
    /// draw. It needs it to *convert*: a pointer position, a hit test and a drag all arrive in the
    /// surface's coordinates, and this is what puts them into the element's own.
    pub fn origin(&self) -> Point<DevicePx, Device> {
        self.content_box.origin
    }

    /// The element's computed `color`, for painting content that follows the text around it.
    pub fn current_color(&self) -> Color {
        self.shape_paint.fill
    }

    /// Fills a rounded rectangle with one colour, as a quad.
    ///
    /// This is the cheap path, the one a background costs, and the reason a custom element
    /// exists at all: retained widgets built from it pay neither a texture nor a rasterisation.
    pub fn fill(&mut self, rect: Rect<DevicePx, Device>, corner_radius: f32, color: Color) {
        let fill = self.rec.color(self.faded(color));
        self.push_quad(rect, corner_radius, fill, [0.0; 4], PaintRef::NONE);
    }

    /// Strokes a rounded rectangle `width` wide with one colour, as a quad.
    pub fn stroke(
        &mut self,
        rect: Rect<DevicePx, Device>,
        corner_radius: f32,
        width: f32,
        color: Color,
    ) {
        let stroke = self.rec.color(self.faded(color));
        self.push_quad(rect, corner_radius, PaintRef::NONE, [width; 4], stroke);
    }

    /// Fills a rounded rectangle with an application's own shader.
    ///
    /// `shader` names an effect the renderer was told about, and `params` is what it draws with.
    /// The rectangle is the effect's whole world: it is given coordinates measured from that
    /// rectangle's own corner, and the framework applies the fragment's clip, its transform and its
    /// folded alpha to whatever the effect returns.
    ///
    /// # What replays
    ///
    /// The parameters are read here, so an element whose revision is unchanged draws the
    /// parameters it drew last time. Anything the effect is meant to follow — a pointer, a
    /// progress value, a phase — has to be part of what the element's revision counts.
    pub fn shade(
        &mut self,
        rect: Rect<DevicePx, Device>,
        corner_radius: f32,
        shader: ShaderId,
        params: ShaderParams,
    ) {
        if !shader.is_some() {
            return;
        }
        let slot = self.rec.scene().shader_params.intern(params);
        let shaded = ShadedQuad::new(self.placed(rect), shader, slot)
            .with_radii(corners(corner_radius))
            .clipped(self.clip)
            .transformed(self.transform)
            .faded(self.alpha);
        self.pushed += usize::from(self.rec.scene().push_shaded(shaded).is_some());
    }

    /// Draws one shape, a path with its own fill and stroke, as vector content.
    ///
    /// The path is in the painter's coordinates. Per-shape paint is the whole shape vocabulary a
    /// vector document has, including [`Ink::Inherited`](zgui_svg::Ink) resolving to the
    /// element's `color`. Dearer than [`fill`](ScenePainter::fill), since a shape is rasterised, so
    /// a widget reaches for it for the geometry quads cannot say.
    pub fn shape(&mut self, shape: &zgui_svg::Shape) {
        // The painter's coordinates start at the content box, so the shape is placed there by a
        // translation and the path itself is not copied.
        let offset = kurbo::Vec2::new(
            f64::from(self.content_box.origin.x.0),
            f64::from(self.content_box.origin.y.0),
        );
        self.shapes_pushed += 1;
        // The fragment's identity in the high bits, the shape's index in the low: stable across
        // frames for the same fragment and distinct within it.
        let id = VectorId((self.vector_id.0 << 16) | (self.shapes_pushed & 0xFFFF));
        let emitted = crate::emit::vector::document::emit_tracked(
            &mut self.rec,
            id,
            &crate::emit::vector::ShapeSource::translated(shape, offset),
            &self.shape_paint,
            self.vector_masks,
            crate::emit::vector::VectorPlacement {
                clip: self.clip,
                transform: self.transform,
                scale: self.scale,
            },
        );
        self.pushed += emitted.pushed;
        if let Some(route) = emitted.route {
            self.vector_routes.insert(route);
        }
    }

    /// Emits one style-uniform run of pre-positioned glyphs.
    ///
    /// `origin` is the line box's top-left corner in painter coordinates. The run's glyph positions
    /// are relative to it, each `y` being the distance down to the baseline, which is what
    /// `Shaper::shape_line` answers in.
    ///
    /// `color` paints the run, and the run's own brush slot is not read: an element that shaped its
    /// own text names its own colour. A colour glyph keeps its own colours and is drawn at
    /// `color`'s alpha. The run asks for per-channel coverage where the text stage would for this
    /// fragment, and is drawn at its variation coordinates and with its synthesis.
    ///
    /// # What replays
    ///
    /// An element whose revision is unchanged does not paint again: its recorded items are
    /// replayed as they stand. So everything read here — the text, the colour, the positions — must
    /// be part of what the element's revision counts, or the screen keeps showing the last frame
    /// this ran on.
    pub fn glyphs(
        &mut self,
        run: &zgui_text::ShapedRun<'_>,
        origin: Point<DevicePx, Device>,
        color: Color,
    ) {
        let paint = self.rec.color(self.faded(color));
        if run.glyphs.is_empty() || paint.kind == compositor_types::PaintKind::NONE {
            return;
        }
        let draw = crate::emit::text::RunDraw {
            origin: [
                self.content_box.origin.x.0 + origin.x.0,
                self.content_box.origin.y.0 + origin.y.0,
            ],
            paint,
            clip: self.rec.clip(self.clip),
            space: self.rec.space(self.transform),
            flags: crate::emit::text::flags(run.size, self.text_subpixel, false),
        };
        crate::emit::text::record(&mut self.rec, run, draw);
        self.pushed += 1;
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

    /// Records one quad over `rect` in painter coordinates, through the fragment's clip and under
    /// its transform.
    fn push_quad(
        &mut self,
        rect: Rect<DevicePx, Device>,
        corner_radius: f32,
        fill: PaintRef,
        border: [f32; 4],
        stroke: PaintRef,
    ) {
        let quad = Quad {
            bounds: local_rect(self.placed(rect)),
            radii: flat_radii(corners(corner_radius)),
            shapes: compositor_types::ROUND,
            border,
            fill,
            stroke,
            paint_origin: [0.0; 2],
            clip: self.rec.clip(self.clip),
            space: self.rec.space(self.transform),
            style: BorderStyle::SOLID,
            _pad: [0; 3],
        };
        self.rec.quad(quad);
        self.pushed += 1;
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

#[cfg(test)]
mod tests {
    use super::ScenePainter;
    use compositor_types::{GlyphFlags, PaintRef};
    use zgui_color::Color;
    use zgui_geom::{DevicePx, Point, Rect, Size};
    use zgui_scene::{ClipId, Scene, SpatialId};
    use zgui_text::{FaceId, ShapedGlyph, ShapedRun};

    use crate::emit::record::{Recorder, Recording};

    /// Two glyphs of one face.
    const GLYPHS: [ShapedGlyph; 2] = [
        ShapedGlyph {
            glyph: 4,
            x: 0.0,
            y: 12.0,
        },
        ShapedGlyph {
            glyph: 5,
            x: 8.0,
            y: 12.0,
        },
    ];

    /// A run of [`GLYPHS`] at one variation instance.
    fn run() -> ShapedRun<'static> {
        ShapedRun {
            face: FaceId(1),
            size: 16.0,
            coords: &[8192],
            synthetic_bold: 0.0,
            synthetic_slant: 0.0,
            brush: zgui_scene::PaintSlot(3),
            glyphs: &GLYPHS,
            text_start: 0,
        }
    }

    /// A painter recording through `rec`, with a content box at a deliberately fractional origin.
    fn painter(rec: Recorder<'_>, subpixel: bool, alpha: f32) -> ScenePainter<'_> {
        ScenePainter {
            rec,
            content_box: Rect::new(
                Point::new(DevicePx(10.5), DevicePx(20.25)),
                Size::new(DevicePx(100.0), DevicePx(40.0)),
            ),
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
            alpha,
            scale: 1.0,
            shape_paint: crate::emit::vector::ShapePaint {
                fill: Color::BLACK,
                stroke: None,
                stroke_width: 0.0,
            },
            vector_masks: &crate::content::vectors::NoVectorMasks,
            text_subpixel: subpixel,
            vector_id: zgui_scene::VectorId(0),
            shapes_pushed: 0,
            vector_routes: crate::emit::vector::VectorRoutes::NONE,
            pushed: 0,
        }
    }

    /// A shape is drawn from its own path, placed at the content box by a translation.
    #[test]
    fn a_custom_shape_is_placed_at_its_content_box() {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(200, 100));
        let mut path = zgui_scene::kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((12.0, 1.0));
        path.line_to((3.0, 9.0));
        path.close_path();
        let shape = zgui_svg::Shape {
            path: std::sync::Arc::new(path),
            fill: Some(zgui_svg::Fill {
                paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Inherited { alpha: 1.0 }),
                rule: zgui_scene::peniko::Fill::NonZero,
            }),
            stroke: None,
            clips: Vec::new(),
        };
        let mut recording = Recording::new();
        let rec = recording.recorder(&mut scene, SpatialId::VIEWPORT, ClipId::ROOT);
        painter(rec, false, 1.0).shape(&shape);

        let item = &recording.paths()[0];
        assert!(
            std::sync::Arc::ptr_eq(&item.path, &shape.path),
            "the painter's own path is drawn, not a copy"
        );
        assert_eq!(
            item.placement,
            zgui_scene::kurbo::Affine::translate((10.5, 20.25))
        );
    }

    /// A run is one glyph run at the content box plus its origin, in the folded colour and at its
    /// own instance.
    #[test]
    fn a_run_is_recorded_at_its_content_box_in_the_folded_colour() {
        let mut scene = Scene::new();
        let mut recording = Recording::new();
        let rec = recording.recorder(&mut scene, SpatialId::VIEWPORT, ClipId::ROOT);
        let mut painter = painter(rec, false, 0.5);
        painter.glyphs(
            &run(),
            Point::new(DevicePx(4.0), DevicePx(2.0)),
            Color::srgb(1.0, 0.0, 0.0, 1.0),
        );
        assert_eq!(painter.pushed, 1);

        let [run] = recording.glyph_runs() else {
            panic!("one run was recorded: {:?}", recording.glyph_runs());
        };
        let origins: Vec<[f32; 2]> = recording
            .glyphs(run)
            .iter()
            .map(|glyph| [glyph.x, glyph.y])
            .collect();
        assert_eq!(origins, [[14.5, 34.25], [22.5, 34.25]]);
        assert_eq!(run.paint, PaintRef::color([128, 0, 0, 128]));
        assert_eq!(recording.coords(run), &[8192]);
    }

    /// The fragment decides whether a run asks for per-channel coverage.
    #[test]
    fn a_run_asks_for_per_channel_coverage_where_the_fragment_allows_it() {
        for subpixel in [false, true] {
            let mut scene = Scene::new();
            let mut recording = Recording::new();
            let rec = recording.recorder(&mut scene, SpatialId::VIEWPORT, ClipId::ROOT);
            painter(rec, subpixel, 1.0).glyphs(
                &run(),
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Color::BLACK,
            );
            let flags = recording.glyph_runs()[0].flags;
            assert_eq!(flags.contains(GlyphFlags::SUBPIXEL), subpixel);
            assert!(!flags.contains(GlyphFlags::SILHOUETTE));
        }
    }
}
