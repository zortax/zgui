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
use zgui_scene::{
    ClipId, ColorSprite, MonoSprite, PaintRef, Quad, Scene, SpatialId, SubpixelSprite, VectorId,
};
use zgui_text::GlyphFormat;

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
    /// Where a run the element shaped itself gets its rasterised tiles.
    pub(crate) glyph_placements: &'a dyn crate::emit::text::GlyphPlacementSource,
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
        let emitted = crate::emit::vector::document::emit_tracked(
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
        self.pushed += emitted.pushed;
        if let Some(route) = emitted.route {
            self.vector_routes.insert(route);
        }
    }

    /// Emits one style-uniform run of pre-positioned glyphs.
    ///
    /// `origin` is the line box's top-left corner in painter coordinates. The run's glyph positions
    /// are relative to it, each `y` being the distance down to the baseline, which is what
    /// [`Shaper::shape_line`](zgui_text_parley::Shaper::shape_line) answers in.
    ///
    /// `color` paints the run, and the run's own brush slot is not read: an element that shaped its
    /// own text names its own colour. A colour face keeps its own colours and is drawn at `color`'s
    /// alpha. Glyphs are drawn from atlas tiles, grayscale or with per-channel coverage the way the
    /// text stage decides it for this fragment. A glyph with no pixels draws nothing.
    ///
    /// # What replays
    ///
    /// An element whose revision is unchanged does not paint again: its recorded primitives are
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

        let color = self.faded(color);
        let format = crate::content::glyphs::format_of(style);
        for glyph in &placed {
            let landed = match format {
                // A colour glyph is a picture rather than a coverage mask, so it carries its own
                // colours and is not tinted.
                GlyphFormat::Color => {
                    let mut sprite =
                        ColorSprite::new(glyph.bounds, glyph.resource).clipped(self.clip);
                    sprite.transform = self.transform.index();
                    sprite.opacity = color.alpha();
                    self.scene.push_color_sprite(sprite).is_some()
                }
                GlyphFormat::Subpixel => {
                    let mut sprite =
                        SubpixelSprite::new(glyph.bounds, glyph.resource, color).clipped(self.clip);
                    sprite.transform = self.transform.index();
                    self.scene.push_subpixel_sprite(sprite).is_some()
                }
                GlyphFormat::Mono => {
                    let mut sprite =
                        MonoSprite::new(glyph.bounds, glyph.resource, color).clipped(self.clip);
                    sprite.transform = self.transform.index();
                    self.scene.push_mono_sprite(sprite).is_some()
                }
            };
            self.pushed += usize::from(landed);
        }
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

#[cfg(test)]
mod tests {
    use super::ScenePainter;
    use core::cell::RefCell;
    use zgui_color::Color;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_scene::{ClipId, Scene, SpatialId};
    use zgui_text::{FaceId, RasterStyle, ShapedGlyph, ShapedRun};

    use crate::emit::text::{GlyphPlacementSource, PlacedGlyph};

    /// A placement source that answers with fixed tiles and records what it was asked.
    #[derive(Default)]
    struct FixedTiles {
        /// How many tiles to answer with.
        tiles: usize,
        /// The style and the origin each call carried.
        asked: RefCell<Vec<(RasterStyle, Point<DevicePx, Device>)>>,
    }

    impl GlyphPlacementSource for FixedTiles {
        fn place_run(
            &self,
            _run: &ShapedRun<'_>,
            style: RasterStyle,
            origin: Point<DevicePx, Device>,
            out: &mut Vec<PlacedGlyph>,
        ) {
            self.asked.borrow_mut().push((style, origin));
            for index in 0..self.tiles {
                out.push(PlacedGlyph {
                    resource: zgui_atlas::AtlasTile {
                        texture: zgui_atlas::TextureId::new(zgui_atlas::TextureKind::Mono, 0),
                        tile: zgui_atlas::TileId(index as u32),
                        bounds: Rect::new(zgui_geom::Point::new(0, 0), zgui_geom::Size::new(6, 9)),
                    }
                    .into(),
                    bounds: Rect::new(
                        Point::new(DevicePx(index as f32 * 8.0), DevicePx(0.0)),
                        Size::new(DevicePx(6.0), DevicePx(9.0)),
                    ),
                });
            }
        }
    }

    /// A run of one glyph over one face.
    const GLYPHS: [ShapedGlyph; 1] = [ShapedGlyph {
        glyph: 4,
        x: 0.0,
        y: 12.0,
    }];

    /// A run, coloured or not.
    fn run(has_color: bool) -> ShapedRun<'static> {
        ShapedRun {
            face: FaceId(1),
            size: 16.0,
            synthetic_bold: 0.0,
            synthetic_slant: 0.0,
            has_color,
            brush: zgui_scene::PaintSlot(3),
            glyphs: &GLYPHS,
        }
    }

    /// A painter over `scene`, with a content box at a deliberately fractional origin.
    fn painter<'a>(
        scene: &'a mut Scene,
        placements: &'a FixedTiles,
        subpixel: bool,
        alpha: f32,
    ) -> ScenePainter<'a> {
        ScenePainter {
            scene,
            content_box: Rect::new(
                Point::new(DevicePx(10.5), DevicePx(20.25)),
                Size::new(DevicePx(100.0), DevicePx(40.0)),
            ),
            clip: ClipId(7),
            transform: SpatialId::VIEWPORT,
            alpha,
            scale: 1.0,
            shape_paint: crate::emit::vector::ShapePaint {
                fill: Color::BLACK,
                stroke: None,
                stroke_width: 0.0,
            },
            vector_masks: &crate::content::vectors::NoVectorMasks,
            glyph_placements: placements,
            text_subpixel: subpixel,
            vector_id: zgui_scene::VectorId(0),
            shapes_pushed: 0,
            vector_routes: crate::emit::vector::VectorRoutes::NONE,
            pushed: 0,
        }
    }

    #[test]
    fn a_grayscale_run_becomes_one_mono_sprite_for_each_tile() {
        let mut scene = Scene::new();
        let placements = FixedTiles {
            tiles: 3,
            ..FixedTiles::default()
        };
        let mut painter = painter(&mut scene, &placements, false, 1.0);
        painter.glyphs(
            &run(false),
            Point::new(DevicePx(4.0), DevicePx(2.0)),
            Color::srgb(1.0, 0.0, 0.0, 1.0),
        );
        let pushed = painter.pushed;

        assert_eq!(pushed, 3, "one primitive per tile");
        assert_eq!(scene.primitives.mono_sprites.len(), 3);
        assert!(scene.primitives.subpixel_sprites.is_empty());
        assert!(scene.primitives.color_sprites.is_empty());
        assert_eq!(placements.asked.borrow()[0].0, RasterStyle::Grayscale);
        for sprite in &scene.primitives.mono_sprites {
            assert_eq!(sprite.clip_id(), ClipId(7));
            assert_eq!(sprite.transform, SpatialId::VIEWPORT.index());
        }
    }

    #[test]
    fn a_subpixel_capable_fragment_asks_for_per_channel_coverage() {
        let mut scene = Scene::new();
        let placements = FixedTiles {
            tiles: 1,
            ..FixedTiles::default()
        };
        let mut painter = painter(&mut scene, &placements, true, 1.0);
        painter.glyphs(
            &run(false),
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Color::srgb(1.0, 0.0, 0.0, 1.0),
        );

        assert_eq!(placements.asked.borrow()[0].0, RasterStyle::Subpixel);
        assert_eq!(scene.primitives.subpixel_sprites.len(), 1);
        assert!(scene.primitives.mono_sprites.is_empty());
    }

    #[test]
    fn a_colour_face_takes_the_colour_path_whatever_the_fragment_says() {
        for subpixel in [false, true] {
            let mut scene = Scene::new();
            let placements = FixedTiles {
                tiles: 1,
                ..FixedTiles::default()
            };
            let mut painter = painter(&mut scene, &placements, subpixel, 0.5);
            painter.glyphs(
                &run(true),
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Color::srgb(1.0, 0.0, 0.0, 1.0),
            );

            assert_eq!(placements.asked.borrow()[0].0, RasterStyle::Color);
            assert_eq!(scene.primitives.color_sprites.len(), 1);
            assert!(scene.primitives.mono_sprites.is_empty());
            assert!(scene.primitives.subpixel_sprites.is_empty());
            // The picture keeps its own colours and is drawn at the folded alpha.
            assert!((scene.primitives.color_sprites[0].opacity - 0.5).abs() < 1.0e-6);
        }
    }

    #[test]
    fn the_origin_handed_on_is_absolute_and_is_split_only_once() {
        let mut scene = Scene::new();
        let placements = FixedTiles {
            tiles: 1,
            ..FixedTiles::default()
        };
        let mut painter = painter(&mut scene, &placements, false, 1.0);
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
        let mut painter = painter(&mut scene, &placements, false, 0.25);
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
        let mut painter = painter(&mut scene, &placements, false, 1.0);
        painter.glyphs(
            &run(false),
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Color::srgb(1.0, 0.0, 0.0, 1.0),
        );

        assert_eq!(painter.pushed, 0);
        assert!(scene.primitives.mono_sprites.is_empty());
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
