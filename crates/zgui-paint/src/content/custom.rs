//! The seam a custom element's paint half plugs into, and the constrained painter it draws with.
//!
//! A custom element's items land at Appendix E step 4, inside its own background and border,
//! before its descendants, under its fragment's clip and transform. That is the same argument the
//! vector arm makes: they are ordered, clipped, moved and faded exactly like a background. What is
//! different is who produces them, and [`ScenePainter`] is the door that keeps that safe: every
//! item goes through the fragment's recorder, under the fragment's clip and transform.

use std::sync::Arc;

use compositor_types::{BorderStyle, Quad};
use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_scene::kurbo;
use zgui_scene::{
    ClipId, ColorSprite, MonoSprite, ShadedQuad, ShaderId, ShaderParams, SpatialId, SubpixelSprite,
    VectorId,
};
use zgui_text::GlyphFormat;

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
/// rounded rectangles as quads, and arbitrary paths as vector content with per-shape paint. No
/// group or layer boundaries, which the walk manages, and no clip creation: an element wanting an
/// inner clip gives a child `overflow: hidden`.
pub struct ScenePainter<'a> {
    /// The fragment's recorder.
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
    /// Where the vector routes keep what they found.
    pub(crate) vector_masks: &'a dyn crate::content::vectors::VectorMaskSource,
    /// Whether per-channel coverage survives this fragment's destination and transform.
    pub(crate) text_subpixel: bool,
    /// The identity the route history of the element's shapes is kept by, from the fragment.
    pub(crate) vector_id: VectorId,
    /// How many shapes have been drawn, so each gets a distinct sub-identity.
    pub(crate) shapes_pushed: u32,
    /// Every route selected by this custom element's shapes.
    pub(crate) vector_routes: crate::emit::vector::VectorRoutes,
    /// How many items were recorded and primitives pushed, reported to the walk.
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
        self.push_quad(
            rect,
            corner_radius,
            fill,
            [0.0; 4],
            compositor_types::PaintRef::NONE,
        );
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
        self.push_quad(
            rect,
            corner_radius,
            compositor_types::PaintRef::NONE,
            [width; 4],
            stroke,
        );
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
        let placed = self.placed(rect);
        let scene = self.rec.scene();
        let slot = scene.shader_params.intern(params);
        let shaded = ShadedQuad::new(placed, shader, slot)
            .with_radii(corners(corner_radius))
            .clipped(self.clip)
            .transformed(self.transform)
            .faded(self.alpha);
        self.pushed += usize::from(scene.push_shaded(shaded).is_some());
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
        let style = run.raster_style(self.text_subpixel);
        // The absolute origin, before anything is split: the phase a glyph is rasterised at follows
        // from where it lands on the surface, and splitting the painter-local position first would
        // rasterise for a phase the glyph is never drawn at.
        let absolute = Point::new(
            DevicePx(self.content_box.origin.x.0 + origin.x.0),
            DevicePx(self.content_box.origin.y.0 + origin.y.0),
        );
        let mut placed = Vec::new();
        self.glyph_placements
            .place_run(run, style, absolute, &mut placed);
        let clip = self.clip;

        let color = self.faded(color);
        let format = crate::content::glyphs::format_of(style);
        let transform = self.transform.index();
        let sprites: Vec<GlyphSprite> = placed
            .iter()
            .map(|glyph| match format {
                // A colour glyph is a picture rather than a coverage mask, so it carries its own
                // colours and is not tinted.
                GlyphFormat::Color => {
                    let mut sprite = ColorSprite::new(glyph.bounds, glyph.resource).clipped(clip);
                    sprite.transform = transform;
                    sprite.opacity = color.alpha();
                    GlyphSprite::Color(sprite)
                }
                GlyphFormat::Subpixel => {
                    let mut sprite =
                        SubpixelSprite::new(glyph.bounds, glyph.resource, color).clipped(clip);
                    sprite.transform = transform;
                    GlyphSprite::Subpixel(sprite)
                }
                GlyphFormat::Mono => {
                    let mut sprite =
                        MonoSprite::new(glyph.bounds, glyph.resource, color).clipped(clip);
                    sprite.transform = transform;
                    GlyphSprite::Mono(sprite)
                }
            })
            .collect();

        // The glyphs of one run sit side by side in one colour, so the scene orders them as one
        // group: one question to its tree for the whole run.
        let inks: Vec<Rect<DevicePx, Device>> = sprites.iter().map(GlyphSprite::ink).collect();
        let scene = self.rec.scene();
        scene.begin_run(&inks, clip, transform);
        for sprite in sprites {
            let landed = match sprite {
                GlyphSprite::Color(sprite) => scene.push_color_sprite(sprite),
                GlyphSprite::Subpixel(sprite) => scene.push_subpixel_sprite(sprite),
                GlyphSprite::Mono(sprite) => scene.push_mono_sprite(sprite),
            };
            self.pushed += usize::from(landed.is_some());
        }
        scene.end_run();
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
        fill: compositor_types::PaintRef,
        border: [f32; 4],
        stroke: compositor_types::PaintRef,
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

/// One glyph of a run, built before the run is pushed.
enum GlyphSprite {
    /// A colour glyph.
    Color(ColorSprite),
    /// A glyph with per-channel coverage.
    Subpixel(SubpixelSprite),
    /// A glyph with one coverage channel.
    Mono(MonoSprite),
}

impl GlyphSprite {
    /// The rectangle the glyph paints.
    fn ink(&self) -> Rect<DevicePx, Device> {
        match self {
            Self::Color(sprite) => sprite.ink(),
            Self::Subpixel(sprite) => sprite.ink(),
            Self::Mono(sprite) => sprite.ink(),
        }
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
    use crate::emit::text::{GlyphPlacementSource, PlacedGlyph};

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

    /// A painter recording into `recording` over `scene`, with a content box at a deliberately
    /// fractional origin.
    fn painter<'a>(
        recording: &'a mut Recording,
        scene: &'a mut Scene,
        placements: &'a FixedTiles,
        subpixel: bool,
        alpha: f32,
    ) -> ScenePainter<'a> {
        let rec: Recorder<'a> = recording.recorder(scene, SpatialId::VIEWPORT, ClipId::ROOT);
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
        let mut painter = painter(&mut recording, &mut scene, &placements, false, 1.0);
        painter.clip = ClipId::ROOT;
        painter.shape(&shape);
        drop(painter);

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
        let placements = FixedTiles {
            tiles: 3,
            ..FixedTiles::default()
        };
        let mut recording = Recording::new();
        let mut painter = painter(&mut recording, &mut scene, &placements, false, 1.0);
        painter.glyphs(
            &run(),
            Point::new(DevicePx(4.0), DevicePx(2.0)),
            Color::srgb(1.0, 0.0, 0.0, 1.0),
        );
        assert_eq!(painter.pushed, 1);

        let [run] = recording.glyph_runs() else {
            panic!("one run was recorded: {:?}", recording.glyph_runs());
        };
        let mut recording = Recording::new();
        let mut painter = painter(&mut recording, &mut scene, &placements, true, 1.0);
        painter.glyphs(
            &run(false),
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Color::srgb(1.0, 0.0, 0.0, 1.0),
        );

        assert_eq!(placements.asked.borrow()[0].0, RasterStyle::Subpixel);
        assert_eq!(scene.primitives.subpixel_sprites.len(), 1);
        assert!(scene.primitives.mono_sprites.is_empty());
    }

    /// The fragment decides whether a run asks for per-channel coverage.
    #[test]
    fn a_run_asks_for_per_channel_coverage_where_the_fragment_allows_it() {
        for subpixel in [false, true] {
            let mut scene = Scene::new();
            let placements = FixedTiles {
                tiles: 1,
                ..FixedTiles::default()
            };
            let mut recording = Recording::new();
            let mut painter = painter(&mut recording, &mut scene, &placements, subpixel, 0.5);
            painter.glyphs(
                &run(true),
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Color::BLACK,
            );
            let flags = recording.glyph_runs()[0].flags;
            assert_eq!(flags.contains(GlyphFlags::SUBPIXEL), subpixel);
            assert!(!flags.contains(GlyphFlags::SILHOUETTE));
        }
    }

    #[test]
    fn the_origin_handed_on_is_absolute_and_is_split_only_once() {
        let mut scene = Scene::new();
        let placements = FixedTiles {
            tiles: 1,
            ..FixedTiles::default()
        };
        let mut recording = Recording::new();
        let mut painter = painter(&mut recording, &mut scene, &placements, false, 1.0);
        painter.glyphs(
            &run(false),
            Point::new(DevicePx(4.0), DevicePx(2.0)),
            Color::srgb(1.0, 0.0, 0.0, 1.0),
        );

        let (_, origin) = placements.asked.borrow()[0];
        assert_eq!(
            origin.x.0, 14.5,
            "the content box's fractional origin reaches the split"
        );
        assert_eq!(origin.y.0, 22.25);
    }

    #[test]
    fn a_group_alpha_is_folded_into_the_colour() {
        let mut scene = Scene::new();
        let placements = FixedTiles {
            tiles: 1,
            ..FixedTiles::default()
        };
        let opaque = Color::srgb(1.0, 0.0, 0.0, 1.0);
        let mut recording = Recording::new();
        let mut painter = painter(&mut recording, &mut scene, &placements, false, 0.25);
        painter.glyphs(
            &run(false),
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            opaque,
        );

        let drawn = scene.primitives.mono_sprites[0].color;
        let faded = opaque.with_alpha(0.25).to_premultiplied_srgb();
        assert_eq!(drawn, faded);
    }

    #[test]
    fn a_run_the_source_placed_nothing_for_draws_nothing() {
        let mut scene = Scene::new();
        let placements = FixedTiles::default();
        let mut recording = Recording::new();
        let mut painter = painter(&mut recording, &mut scene, &placements, false, 1.0);
        painter.glyphs(
            &run(false),
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Color::srgb(1.0, 0.0, 0.0, 1.0),
        );

        assert_eq!(painter.pushed, 0);
        assert!(scene.primitives.mono_sprites.is_empty());
    }

    #[test]
    fn a_run_of_glyphs_stands_above_its_band_at_one_order_without_overlaps() {
        let mut scene = Scene::new();
        scene.begin_frame(zgui_geom::Size::new(400, 400));
        let band = scene.paints.solid(Color::srgb(0.2, 0.2, 0.2, 1.0));
        scene.push_quad(zgui_scene::Quad::filled(
            Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(200.0), DevicePx(60.0)),
            ),
            zgui_scene::PaintRef::solid(band),
        ));
        let placements = FixedTiles {
            tiles: 4,
            ..FixedTiles::default()
        };
        let mut recording = Recording::new();
        let mut painter = painter(&mut recording, &mut scene, &placements, false, 1.0);
        painter.clip = ClipId::ROOT;
        painter.glyphs(
            &run(false),
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Color::srgb(1.0, 1.0, 1.0, 1.0),
        );

        let orders: Vec<_> = scene
            .primitives
            .mono_sprites
            .iter()
            .map(|sprite| sprite.order)
            .collect();
        assert_eq!(
            orders,
            [2, 2, 2, 2],
            "the run stands above the band, side by side"
        );
        assert_eq!(scene.order_overlaps().map(|found| found.len()), Ok(0));
    }
}
