//! Text: the glyphs of a line, the lines that decorate them, and the shadows behind both.
//!
//! # Where the glyphs come from
//!
//! This stage does not shape and does not rasterise. A fragment says *which line of which
//! paragraph* it draws, and [`GlyphSource`] is the seam that turns that into positioned, already
//! rasterised glyphs. Keeping it a seam rather than a dependency is what lets the whole paint stage
//! be exercised with no font files: a test supplies its own glyphs, and the code under test is the
//! same code.
//!
//! The seam is a visitor rather than a slice-returning method on purpose. A run's glyphs live
//! wherever the text engine put them, and a caller that had to hand out a slice would either have
//! to own one or allocate one per line, every frame.
//!
//! # Which sprite a run becomes
//!
//! Three-channel coverage is conditional in two independent ways and both are decided here, because
//! here is where both answers are known. The device may not have dual-source blending at all; and
//! the coverage *is* the blend factor, which is meaningless against a destination that is not
//! opaque — so a run landing inside a group's own target is demoted whatever the device can do.

use std::sync::Arc;

use zgui_color::Color;
use zgui_css::parity::Support;
use zgui_css::values::color::{current, resolve};
use zgui_css::{ComputedStyle, register_properties};
use zgui_geom::{Affine2, Device, DevicePx, Point, Rect, Size};
use zgui_layout::fragment::ParagraphId;
use zgui_scene::kurbo::{Affine, BezPath, Shape};
use zgui_scene::prim::decoration::DecorationStyle as SceneDecorationStyle;
use zgui_scene::{
    ClipId, ClipLink, ColorSprite, Decoration, MonoSprite, Paint, PaintRef, PaintSlot, Resource,
    Scene, SpatialId, SubpixelSprite, VectorId, VectorItem,
};
use zgui_text::{GlyphFormat, RunSurface};

use crate::content::glyphs::OutlineGlyph;

register_properties! {
    text_decoration_line      => Support::Implemented("zgui-paint::emit::text"),
    text_decoration_color     => Support::Implemented("zgui-paint::emit::text"),
    text_decoration_style     => Support::Implemented("zgui-paint::emit::text"),
}

/// One glyph, rasterised and placed on the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    /// The glyph's pixels: where they are, or what they are called.
    pub resource: Resource,
    /// Where those pixels land, in absolute device pixels.
    pub bounds: Rect<DevicePx, Device>,
}

/// What a run's glyphs arrived as, which is decided by the path the run takes.
///
/// The two arms are the two halves of [`RasterPath`](zgui_text::RasterPath): a source has already
/// asked the run which one it is and answered in kind, so nothing downstream chooses again.
#[derive(Clone, Copy, Debug)]
pub enum RunContent<'a> {
    /// Coverage tiles, drawn as one quad per glyph.
    Tiles(&'a [PlacedGlyph]),
    /// Curves, filled by the frame's path rasteriser.
    Outlines(&'a [OutlineGlyph]),
}

/// One style-uniform run of glyphs within a line.
#[derive(Clone, Copy, Debug)]
pub struct GlyphRun<'a> {
    /// The glyphs, in logical order, in whichever form the run takes.
    pub content: RunContent<'a>,
    /// How the tiles are laid out, which decides which sprite kind the run becomes.
    ///
    /// Meaningless for an outline run, which has no tiles: a curve is filled with a brush rather
    /// than sampled as coverage.
    pub format: GlyphFormat,
    /// The brush slot the run's colour is read from.
    ///
    /// A slot rather than a colour, because a shaped paragraph outlives the frame that produced it:
    /// a theme change rewrites the slot and re-colours every cached paragraph without re-shaping a
    /// single string.
    pub paint: PaintSlot,
    /// How much a synthesised bold thickens a stem, in device pixels.
    ///
    /// Zero for the atlas path, which has the thickening in its coverage already. An outline run
    /// carries it because emboldening a curve is a stroke around it, and the curve the face draws
    /// is the same curve either way.
    pub synthetic_bold: f32,
}

/// Where one line's glyphs are wanted, and in what form.
///
/// Both fields are decisions the emitter has already made and a source cannot make for itself. The
/// origin is where the line box landed, which comes from the fragment tree; whether per-channel
/// coverage is wanted depends on the device and on the target the run lands in, neither of which a
/// text engine knows about. Handing both down is what lets a source rasterise *once*, in the form
/// that will actually be drawn, rather than rasterising subpixel coverage that then has to be
/// drawn as a mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphRequest {
    /// The line box's top-left corner, in absolute device pixels.
    pub origin: Point<DevicePx, Device>,
    /// Whether the glyphs may be rasterised with one coverage value per colour channel.
    pub subpixel: bool,
    /// What the surface does to the run: the transform in force and the brush it is painted with.
    ///
    /// The other half of what decides [`RasterPath`](zgui_text::RasterPath); the run itself carries
    /// the rest. A source
    /// puts the two together and answers in tiles or in curves — which is why the choice cannot be
    /// made downstream of here, where the answer has already been rasterised one way or the other.
    pub surface: RunSurface,
}

/// Where a text fragment's glyphs come from.
///
/// An implementation answers for the lines it knows about and says nothing for the rest; a line
/// with no glyphs — a blank one, or one whose paragraph has not been shaped — visits nothing, which
/// is the same answer as a line of spaces and is drawn the same way.
pub trait GlyphSource {
    /// Visits each style-uniform run of one line, in the order the runs are drawn.
    ///
    /// The bounds a run reports are absolute: a source is given where the line box is and places
    /// its glyphs against it, so nothing downstream has to know how a text engine numbers its own
    /// coordinates.
    fn visit_line(
        &self,
        paragraph: ParagraphId,
        line: u16,
        request: GlyphRequest,
        visit: &mut dyn FnMut(GlyphRun<'_>),
    );
}

/// A glyph source with nothing in it.
///
/// This is what a document with no text is painted through, and what a test that is not about text
/// uses so that it does not have to pretend to have a font engine.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoGlyphs;

impl GlyphSource for NoGlyphs {
    fn visit_line(
        &self,
        _paragraph: ParagraphId,
        _line: u16,
        _request: GlyphRequest,
        _visit: &mut dyn FnMut(GlyphRun<'_>),
    ) {
    }
}

/// Where a run the caller shaped itself gets its rasterised tiles.
///
/// [`GlyphSource`] answers for the lines a paragraph was broken into, and is asked by paragraph and
/// line number. This answers for a run its caller shaped — a custom element drawing its own text —
/// and is asked with the run itself.
///
/// The placements are written into the caller's own vector rather than visited, because the source
/// holds the glyph cache and the atlas while it answers and whoever draws holds the scene. Handing
/// back an owned answer closes the first borrow before the second is needed.
pub trait GlyphPlacementSource {
    /// Places `run`'s glyphs with the line box's top-left corner at `origin`, rasterising and
    /// uploading whatever is not cached yet.
    ///
    /// `origin` is absolute device pixels on the surface, because the phase a glyph is rasterised
    /// at is a property of where it lands there. A glyph with no pixels — a space — and a glyph the
    /// atlas has no room for are both left out, so the answer may be shorter than the run.
    fn place_run(
        &self,
        run: &zgui_text::ShapedRun<'_>,
        style: zgui_text::RasterStyle,
        origin: Point<DevicePx, Device>,
        out: &mut Vec<PlacedGlyph>,
    );
}

/// A placement source with no atlas behind it.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoGlyphPlacements;

impl GlyphPlacementSource for NoGlyphPlacements {
    fn place_run(
        &self,
        _run: &zgui_text::ShapedRun<'_>,
        _style: zgui_text::RasterStyle,
        _origin: Point<DevicePx, Device>,
        _out: &mut Vec<PlacedGlyph>,
    ) {
    }
}

/// Which decoration lines a box draws over the text inside it, and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecorationStyle {
    /// Whether a line is drawn under the text.
    pub underline: bool,
    /// Whether a line is drawn over it.
    pub overline: bool,
    /// Whether a line is drawn through it.
    pub line_through: bool,
    /// How the lines are drawn.
    pub style: SceneDecorationStyle,
    /// The lines' colour.
    pub color: Color,
    /// The lines' thickness in device pixels.
    pub thickness: f32,
}

impl Default for DecorationStyle {
    fn default() -> Self {
        Self {
            underline: false,
            overline: false,
            line_through: false,
            style: SceneDecorationStyle::Solid,
            color: Color::BLACK,
            thickness: 1.0,
        }
    }
}

impl DecorationStyle {
    /// Lowers a style's text decoration.
    pub fn of(style: &ComputedStyle, scale: f32) -> Self {
        use zgui_css::values::text::TextDecorationLineValue as Line;
        let text = style.get_text();
        let lines = text.text_decoration_line;
        Self {
            underline: lines.contains(Line::UNDERLINE),
            overline: lines.contains(Line::OVERLINE),
            line_through: lines.contains(Line::LINE_THROUGH),
            style: line_style(text.text_decoration_style),
            color: resolve(&text.text_decoration_color, current(style)),
            // `text-decoration-thickness` is not a property this engine build generates, so there
            // is nothing to read: one device pixel is what every engine draws at reading sizes, and
            // it is what the ink audit is written against.
            thickness: scale.max(1.0),
        }
    }

    /// Whether any line is drawn at all.
    pub fn draws_anything(&self) -> bool {
        (self.underline || self.overline || self.line_through) && self.color.alpha() != 0.0
    }

    /// How far above and below its own stroke this decoration's shape reaches, in device pixels.
    ///
    /// Zero for the three shapes that *are* a stroke. A wave swings about the stroke and a doubled
    /// line is two strokes with a stroke's gap between them, so both reach one stroke either side
    /// of it.
    pub fn reach(&self) -> f32 {
        if !self.draws_anything() {
            return 0.0;
        }
        match self.style {
            SceneDecorationStyle::Wavy | SceneDecorationStyle::Double => self.thickness,
            _ => 0.0,
        }
    }

    /// How tall the rectangle this line is drawn in has to be, in device pixels.
    ///
    /// The shader evaluates a wave and a doubled line across the whole rectangle they are given
    /// rather than across their stroke — the wave's amplitude is what is left of the height once
    /// the stroke is drawn, and the doubled line's two strokes are each a third of it. Handed a
    /// rectangle one stroke tall, which is what a solid line wants, a wave flattens into a straight
    /// line and a doubled line collapses into a single blurred one: drawn, and indistinguishable
    /// from the decoration it is not.
    pub fn band(&self) -> f32 {
        self.thickness + self.reach() * 2.0
    }

    /// Folds this into a content hash, for the lowering cache's fallback lookup.
    pub fn fold_into(&self, hash: zgui_scene::ContentHash) -> zgui_scene::ContentHash {
        hash.u32(u32::from(self.underline))
            .u32(u32::from(self.overline))
            .u32(u32::from(self.line_through))
            .u32(self.style as u32)
            .u32(self.color.space() as u32)
            .f32s(&self.color.components())
            .f32(self.color.alpha())
            .f32(self.thickness)
    }
}

/// The scene's spelling of a `text-decoration-style` keyword.
fn line_style(style: zgui_css::values::text::TextDecorationStyleValue) -> SceneDecorationStyle {
    use zgui_css::values::text::TextDecorationStyleValue as Value;
    match style {
        Value::Solid => SceneDecorationStyle::Solid,
        Value::Double => SceneDecorationStyle::Double,
        Value::Dotted => SceneDecorationStyle::Dotted,
        Value::Dashed => SceneDecorationStyle::Dashed,
        Value::Wavy => SceneDecorationStyle::Wavy,
        Value::MozNone => SceneDecorationStyle::Solid,
    }
}

/// Where a text fragment's primitives are drawn.
#[derive(Clone, Copy, Debug)]
pub struct TextPlacement {
    /// The line box, in absolute device pixels.
    pub line: Rect<DevicePx, Device>,
    /// The chain the run is drawn through.
    pub clip: ClipId,
    /// The transform it is drawn under.
    pub transform: SpatialId,
    /// Whether the target the run lands in is opaque.
    pub opaque_target: bool,
    /// Whether the device can antialias per colour channel at all.
    pub subpixel_capable: bool,
    /// Whether the run is drawn with no transform over it.
    ///
    /// Subpixel coverage is three side-by-side answers about thirds of one physical pixel, and it
    /// survives only when the sprite lands on the pixels it was rasterised for. A turned or scaled
    /// run is resampled on its way to the surface, which smears the per-channel stripes into
    /// coloured fringes — so a transformed run is drawn with whole-pixel coverage instead, which
    /// is what every browser does to text on a composited transform.
    pub upright: bool,
    /// How many device pixels one CSS pixel is, for a brush whose ramp is measured in lengths.
    pub scale: f32,
    /// Where this line was cut off, when it reaches past its box and the box marks the cut.
    pub ellipsis: Option<EllipsisPaint>,
}

/// Where a line was cut off, and what marks the cut.
///
/// Decided while the line was laid out, because a cut falls on a *cluster* boundary and glyph runs
/// carry no text offsets — nothing here could work out where one is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipsisPaint {
    /// The paragraph the mark's glyphs are a paragraph of.
    pub paragraph: ParagraphId,
    /// The device x-coordinate the line is cut at.
    pub cutoff: f32,
    /// How wide the mark is.
    pub width: f32,
    /// Whether the cut is at the line's start, so the mark is drawn to the left of it.
    pub at_start: bool,
}

impl TextPlacement {
    /// Whether a subpixel run may stay a subpixel run.
    ///
    /// All three are needed and none implies the others: a device without dual-source blending
    /// has no pipeline to draw one with, a destination that is not opaque has no meaning for
    /// per-channel coverage whatever the device can do, and a transformed run is resampled out of
    /// the pixel grid its stripes were measured against.
    pub fn keeps_subpixel(&self) -> bool {
        self.subpixel_capable && self.opaque_target && self.upright
    }

    /// The two-dimensional transform in force, or the identity for one that leaves the plane.
    ///
    /// A three-dimensional transform is flattened rather than refused, and that matches what the
    /// rasteriser does with the same matrix: what is lost is the transform, never the run.
    fn affine(&self, scene: &Scene) -> Affine2 {
        scene
            .spatial
            .resolve(self.transform)
            .as_ref()
            .and_then(zgui_geom::Matrix4::to_affine2)
            .unwrap_or(Affine2::IDENTITY)
    }

    /// What the surface does to a run drawn through this placement with `brush`.
    fn surface(&self, scene: &Scene, brush: &RunBrush) -> RunSurface {
        RunSurface {
            translated_only: self.affine(scene).is_translation(),
            solid_brush: matches!(brush, RunBrush::Solid),
        }
    }
}

/// What paints a run's glyphs.
///
/// Two arms because they are two different mechanisms rather than two values of one: a colour is
/// multiplied into a coverage tile by the sprite shader, and a paint is a brush a path rasteriser
/// evaluates across the curve it fills. Only the first can reach the atlas, which is why a run
/// carrying the second is promoted before it is rasterised at all.
#[derive(Clone, Copy, Debug)]
enum RunBrush {
    /// One colour, read from the run's own brush slot and falling back to the element's `color`.
    Solid,
    /// A paint resolved against the line box — a gradient across the text.
    Painted(PaintRef),
}

/// What the boxes *above* a line contributed to it.
///
/// Neither of these can be read off the line's own style. A line box belongs to an anonymous inline
/// root generated below the element that declared them, and neither `text-decoration` nor
/// `background-image` is an inherited property — so both are propagated down the emit walk and
/// handed in here. Reading the fragment's own style instead is a decoration that draws nothing and
/// a ramp that paints nothing, under declarations that are perfectly correct.
#[derive(Clone, Copy, Default)]
pub struct Inherited<'a> {
    /// The ramp painting the glyphs, if a box above them asked for one.
    pub text_fill: Option<&'a crate::lower::background::GradientSpec>,
    /// The decorations drawn across the line, outermost first.
    pub decorations: &'a [DecorationStyle],
}

/// Emits one line's glyphs and decorations, and returns how many primitives were pushed.
///
/// The order is CSS's: the shadows behind everything, then the glyphs, then the decoration lines
/// over them.
///
/// `decorations` is the whole list in force at this line rather than the line's own box's value,
/// because a decoration belongs to the box that declared it and is drawn across every in-flow
/// descendant of that box. A line box is generated by an anonymous inline root below the element
/// that carried the declaration, so its own style says nothing about it.
pub fn emit(
    scene: &mut Scene,
    glyphs: &dyn GlyphSource,
    paragraph: ParagraphId,
    line: u16,
    style: &crate::lower::PaintStyle,
    inherited: Inherited<'_>,
    placement: TextPlacement,
) -> usize {
    let Inherited {
        text_fill,
        decorations,
    } = inherited;
    // Resolved once for the whole line rather than per pass: a gradient across the text is
    // measured across the line box, and resolving it again per shadow would intern the same ramp
    // several times a frame.
    let brush = match text_fill {
        Some(spec) => {
            match crate::emit::vector::gradient_for_vector(
                scene,
                spec,
                placement.line,
                placement.scale,
            ) {
                Some(reference) => RunBrush::Painted(reference),
                None => RunBrush::Solid,
            }
        }
        None => RunBrush::Solid,
    };
    // The path the run takes is settled before the first pass, and every pass takes it: a shadow
    // drawn from tiles under text drawn from curves is the same silhouette placed by two different
    // rules, and the two disagree by whatever the tile path rounded.
    let surface = placement.surface(scene, &brush);
    let mut pushed = 0;
    let mut pass = 0;
    // The mark goes on under the line's own clip; the line's glyphs go on under a tighter one that
    // stops where the content was cut. Both are decided before the first pass, so a shadow and the
    // text it shadows are cut at the same place.
    let mark = placement.ellipsis;
    let original = placement;
    let placement = cut_to_ellipsis(scene, placement);
    for shadow in &style.text_shadows {
        if shadow.is_invisible() {
            continue;
        }
        let offset = Size::new(DevicePx(shadow.offset_x), DevicePx(shadow.offset_y));
        pass += 1;
        pushed += runs(
            scene,
            glyphs,
            Where {
                paragraph,
                line,
                pass,
            },
            placement,
            surface,
            // A shadowed copy is one colour whatever paints the text itself: a shadow is a
            // silhouette, and a gradient inside one is a gradient nobody can see.
            Painting {
                tint: shadow.color,
                brush: RunBrush::Solid,
                offset,
                force_mono: true,
            },
        );
    }
    pushed += runs(
        scene,
        glyphs,
        Where {
            paragraph,
            line,
            pass: 0,
        },
        placement,
        surface,
        Painting {
            tint: style.color,
            brush,
            offset: Size::new(DevicePx(0.0), DevicePx(0.0)),
            force_mono: false,
        },
    );
    for decoration in decorations {
        pushed += lines(scene, decoration, placement);
    }
    if let Some(mark) = mark {
        pushed += runs(
            scene,
            glyphs,
            Where {
                paragraph: mark.paragraph,
                line: 0,
                pass: 0,
            },
            // Drawn through the line's *untightened* clip, at the boundary the cut fell on. Its own
            // box still clips it, which is what keeps a mark wider than the box from spilling out.
            TextPlacement {
                line: mark.rect(placement.line),
                ellipsis: None,
                ..original
            },
            surface,
            Painting {
                tint: style.color,
                brush: RunBrush::Solid,
                offset: Size::new(DevicePx(0.0), DevicePx(0.0)),
                force_mono: false,
            },
        );
    }
    pushed
}

impl EllipsisPaint {
    /// The rectangle the mark's own glyphs are placed in, given the line it marks.
    ///
    /// The same line box moved to the cut: a mark is one line of one paragraph, drawn on the
    /// baseline the line it marks sits on.
    fn rect(&self, line: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
        let x = if self.at_start {
            self.cutoff - self.width
        } else {
            self.cutoff
        };
        Rect::new(
            Point::new(DevicePx(x), line.origin.y),
            Size::new(DevicePx(self.width), line.size.height),
        )
    }
}

/// The placement a cut line's own glyphs are drawn through.
///
/// The same placement under a tighter clip, so everything past the cut goes undrawn. A clip, and
/// never a filter over the glyphs: a glyph's ink may reach past its own advance, so dropping glyphs
/// by position cuts the ink of a cluster that survived or keeps the ink of a hidden one, and the
/// specification speaks of characters.
fn cut_to_ellipsis(scene: &mut Scene, placement: TextPlacement) -> TextPlacement {
    let Some(mark) = placement.ellipsis else {
        return placement;
    };
    let line = placement.line;
    let (left, right) = if mark.at_start {
        (mark.cutoff, line.origin.x.0 + line.size.width.0)
    } else {
        (line.origin.x.0, mark.cutoff)
    };
    let window = Rect::new(
        Point::new(DevicePx(left), line.origin.y),
        Size::new(DevicePx((right - left).max(0.0)), line.size.height),
    );
    TextPlacement {
        clip: scene.clips.push(placement.clip, ClipLink::rect(window)),
        ..placement
    }
}

/// Which line is being drawn, and which of its passes this is.
///
/// The pass number is what keeps a shadow's curves from claiming the identity the text's own
/// curves are cached under: two passes over one line draw the same glyphs at different places, and
/// a rasteriser that held one encoding for both would re-encode every one of them, every frame.
#[derive(Clone, Copy, Debug)]
struct Where {
    /// The paragraph the line belongs to.
    paragraph: ParagraphId,
    /// The line within it.
    line: u16,
    /// Which pass over that line this is: zero for the text, one and up for its shadows.
    pass: u32,
}

/// What one pass over a line paints with.
#[derive(Clone, Copy, Debug)]
struct Painting {
    /// The colour to use where the run's own brush slot says nothing, and the whole colour of a
    /// shadow pass.
    tint: Color,
    /// What fills the glyphs.
    brush: RunBrush,
    /// How far this pass is displaced from the text itself.
    offset: Size<DevicePx, Device>,
    /// Whether this pass is a silhouette in one colour rather than the text itself.
    force_mono: bool,
}

/// Emits one pass over one line's glyphs, and returns how many primitives were pushed.
///
/// The tint is the run's own brush unless one is given, which is what makes a shadow a second pass
/// over the same glyphs rather than a second set of them.
fn runs(
    scene: &mut Scene,
    glyphs: &dyn GlyphSource,
    at: Where,
    placement: TextPlacement,
    surface: RunSurface,
    painting: Painting,
) -> usize {
    let request = GlyphRequest {
        origin: placement.line.origin,
        // A shadowed copy is coverage tinted with one colour, so it is never asked for per-channel
        // coverage: subpixel antialiasing of a blurred silhouette is fringing.
        subpixel: placement.keeps_subpixel() && !painting.force_mono,
        surface,
    };
    // Collected rather than drawn as they are visited: a source holds the atlas and the glyph
    // cache open for the length of the visit, and pushing into the scene inside it would need both
    // at once.
    let mut emitted: Vec<Emitted> = Vec::new();
    glyphs.visit_line(at.paragraph, at.line, request, &mut |run| {
        emitted.push(Emitted {
            format: run.format,
            slot: run.paint,
            synthetic_bold: run.synthetic_bold,
            content: match run.content {
                RunContent::Tiles(tiles) => Owned::Tiles(tiles.to_vec()),
                RunContent::Outlines(outlines) => Owned::Outlines(outlines.to_vec()),
            },
        });
    });
    let mut pushed = 0;
    let mut index = 0;
    for run in emitted {
        let color = if painting.force_mono {
            painting.tint
        } else {
            brush(scene, run.slot, painting.tint)
        };
        match &run.content {
            Owned::Tiles(tiles) => {
                for glyph in tiles {
                    pushed += usize::from(tile(
                        scene,
                        placement,
                        Tiled {
                            format: run.format,
                            color,
                            offset: painting.offset,
                            subpixel: request.subpixel,
                            force_mono: painting.force_mono,
                        },
                        glyph,
                    ));
                    index += 1;
                }
            }
            Owned::Outlines(outlines) => {
                // A colour run never reaches here — it has no single outline to fill — so a
                // shadow pass over one has nothing to suppress, unlike the tile path.
                let fill = match painting.brush {
                    RunBrush::Painted(reference) if !painting.force_mono => reference,
                    _ => scene.paints.add(Paint::Solid(color)),
                };
                let affine = placement.affine(scene);
                for glyph in outlines {
                    pushed += usize::from(outline(
                        scene,
                        placement,
                        affine,
                        Outlined {
                            id: outline_id(at, index),
                            fill,
                            stroke: run.synthetic_bold,
                            offset: painting.offset,
                        },
                        glyph,
                    ));
                    index += 1;
                }
            }
        }
    }
    pushed
}

/// One visited run, held until the source's borrow of the atlas has been given up.
struct Emitted {
    /// How the tiles are laid out.
    format: GlyphFormat,
    /// The run's brush slot.
    slot: PaintSlot,
    /// How much a synthesised bold thickens a stem, in device pixels.
    synthetic_bold: f32,
    /// The glyphs.
    content: Owned,
}

/// A visited run's glyphs, owned.
enum Owned {
    /// Coverage tiles.
    Tiles(Vec<PlacedGlyph>),
    /// Curves.
    Outlines(Vec<OutlineGlyph>),
}

/// Everything one tiled glyph is drawn with.
#[derive(Clone, Copy, Debug)]
struct Tiled {
    /// How the tile's bytes are laid out.
    format: GlyphFormat,
    /// What the coverage is multiplied by.
    color: Color,
    /// How far this pass is displaced from the text itself.
    offset: Size<DevicePx, Device>,
    /// Whether per-channel coverage may be drawn as such.
    subpixel: bool,
    /// Whether this pass is a silhouette in one colour rather than the text itself.
    force_mono: bool,
}

/// Draws one glyph's tile, and says whether it landed.
fn tile(scene: &mut Scene, placement: TextPlacement, drawn: Tiled, glyph: &PlacedGlyph) -> bool {
    let Tiled {
        format,
        color,
        offset,
        subpixel,
        force_mono,
    } = drawn;
    let bounds = glyph.bounds.translate(offset);
    let landed = match format {
        // A colour glyph is a picture rather than a coverage mask, so it carries its own colour
        // and is not tinted — and a shadow of one has no silhouette to draw, because the alpha it
        // would be cut from is the picture's own rather than the glyph's.
        GlyphFormat::Color if force_mono => None,
        GlyphFormat::Color => {
            let mut sprite = ColorSprite::new(bounds, glyph.resource).clipped(placement.clip);
            sprite.transform = placement.transform.index();
            sprite.opacity = color.alpha();
            scene.push_color_sprite(sprite)
        }
        GlyphFormat::Subpixel if subpixel => {
            let mut sprite =
                SubpixelSprite::new(bounds, glyph.resource, color).clipped(placement.clip);
            sprite.transform = placement.transform.index();
            scene.push_subpixel_sprite(sprite)
        }
        // A source that answered with per-channel coverage where none was asked for is drawn
        // as a mask rather than trusted: the coverage *is* the blend factor, and against a
        // destination that is not opaque there is nothing for it to be a factor of.
        GlyphFormat::Subpixel | GlyphFormat::Mono => {
            let mut sprite = MonoSprite::new(bounds, glyph.resource, color).clipped(placement.clip);
            sprite.transform = placement.transform.index();
            scene.push_mono_sprite(sprite)
        }
    };
    landed.is_some()
}

/// Everything one outlined glyph is drawn with.
#[derive(Clone, Copy, Debug)]
struct Outlined {
    /// The identity a rasteriser caches this glyph's encoding under.
    id: VectorId,
    /// What fills the curve.
    fill: PaintRef,
    /// How wide a stroke stands in for a weight the face does not have; zero for no stroke.
    stroke: f32,
    /// How far this pass is displaced from the text itself.
    offset: Size<DevicePx, Device>,
}

/// Fills one glyph's curves, and says whether the item landed.
///
/// The ink is the curve's bounds **after** the run's transform, because that is the rectangle the
/// rasteriser will write: the item is drawn into a scratch covering exactly this rectangle and
/// composited back through it, so an ink measured before the transform would cut a turned letter
/// off at the edge of the box it would have occupied upright.
fn outline(
    scene: &mut Scene,
    placement: TextPlacement,
    affine: Affine2,
    drawn: Outlined,
    glyph: &OutlineGlyph,
) -> bool {
    let path = if drawn.offset.width.0 == 0.0 && drawn.offset.height.0 == 0.0 {
        Arc::clone(&glyph.path)
    } else {
        let mut moved = BezPath::clone(&glyph.path);
        moved.apply_affine(Affine::translate((
            f64::from(drawn.offset.width.0),
            f64::from(drawn.offset.height.0),
        )));
        Arc::new(moved)
    };
    let bounds = path.bounding_box();
    let reach = f64::from(drawn.stroke) * 0.5;
    let local = Rect::from_corners(
        Point::new(
            DevicePx((bounds.x0 - reach) as f32),
            DevicePx((bounds.y0 - reach) as f32),
        ),
        Point::new(
            DevicePx((bounds.x1 + reach) as f32),
            DevicePx((bounds.y1 + reach) as f32),
        ),
    );
    let mut item = VectorItem::filled(drawn.id, path, drawn.fill).clipped(placement.clip);
    item.ink = affine.transform_rect(local);
    item.local_ink = local;
    item.transform = Some(placement.transform);
    if drawn.stroke > 0.0 {
        // A synthesised bold is a stroke around the face's own curve, in the same brush: the
        // curve is what the face draws and the weight is what it does not have.
        item.stroke = Some(zgui_scene::VectorStroke::solid(drawn.fill, drawn.stroke));
    }
    scene.push_vector(item).is_some()
}

/// The identity one outlined glyph's curves are cached under.
///
/// Derived from where the glyph is in the document rather than from what it is, so that the same
/// letter twice on a line is two entries and one letter that moved is still one — a rasteriser
/// re-encodes when the geometry under an identity changes, which is exactly what a moved glyph
/// wants and what a shared identity for two glyphs would make happen twice a frame.
fn outline_id(at: Where, index: u32) -> VectorId {
    let mut hash = zgui_scene::ContentHash::new();
    hash = hash
        .u32(at.paragraph.0)
        .u32(u32::from(at.line))
        .u32(at.pass)
        .u32(index);
    VectorId(hash.finish() as u32)
}

/// The colour a run is drawn in, read from its brush slot and falling back to the element's own.
///
/// A slot that resolves to nothing is a paragraph shaped before the brush was registered; drawing
/// it in the element's `color` is right far more often than drawing it in whatever was in slot zero.
fn brush(scene: &Scene, slot: PaintSlot, fallback: Color) -> Color {
    match scene.text_paints.get(slot) {
        Some(paint) => Color::srgb(
            paint.color[0],
            paint.color[1],
            paint.color[2],
            paint.color[3],
        ),
        None => fallback,
    }
}

/// Emits one contributed decoration's lines across one line box.
///
/// The three positions are the bottom, the top and the middle of the line box. The text engine
/// reports a font's own underline position, which is finer; this is where each engine puts a
/// decoration when the face declines to say.
///
/// Each is drawn in a band that is clamped to the line box. A wave and a doubled line need a band
/// several strokes tall to have a shape at all, and letting that band hang outside the line box
/// would put ink outside the rectangle the fragment reports — which is what leaves a trail behind
/// when the text moves. Clamping costs a wave some amplitude on a tight line and nothing at all on
/// an ordinary one, where the half-leading is already taller than the band.
fn lines(scene: &mut Scene, style: &DecorationStyle, placement: TextPlacement) -> usize {
    if !style.draws_anything() {
        return 0;
    }
    let line = placement.line;
    let band = style.band().min(line.size.height.0.max(style.thickness));
    let mut pushed = 0;
    let mut draw = |centre: f32| {
        let top =
            (centre - band * 0.5).clamp(line.top().0, (line.bottom().0 - band).max(line.top().0));
        let rect = Rect::new(
            Point::new(line.origin.x, DevicePx(top)),
            Size::new(line.size.width, DevicePx(band)),
        );
        let mut drawn = Decoration::new(rect, style.thickness, style.color, style.style)
            .clipped(placement.clip);
        drawn.transform = placement.transform.index();
        let landed = scene.push_decoration(drawn);
        pushed += usize::from(landed.is_some());
    };
    if style.underline {
        draw(line.bottom().0 - style.thickness * 0.5);
    }
    if style.overline {
        draw(line.top().0 + style.thickness * 0.5);
    }
    if style.line_through {
        draw(line.origin.y.0 + line.size.height.0 * 0.5);
    }
    pushed
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_scene::prim::decoration::DecorationStyle as SceneDecorationStyle;

    use super::{DecorationStyle, TextPlacement};
    use zgui_geom::{DevicePx, Point, Rect, Size};
    use zgui_scene::{ClipId, SpatialId};

    /// A placement on an opaque target, on a device that can do subpixel text.
    fn placement() -> TextPlacement {
        TextPlacement {
            line: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(64.0), DevicePx(16.0)),
            ),
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
            opaque_target: true,
            subpixel_capable: true,
            upright: true,
            scale: 1.0,
            ellipsis: None,
        }
    }

    #[test]
    fn subpixel_needs_a_capable_device_an_opaque_target_and_no_transform() {
        assert!(placement().keeps_subpixel());
        assert!(
            !TextPlacement {
                opaque_target: false,
                ..placement()
            }
            .keeps_subpixel(),
            "per-channel coverage against a transparent destination is meaningless"
        );
        assert!(
            !TextPlacement {
                subpixel_capable: false,
                ..placement()
            }
            .keeps_subpixel(),
            "a device with no dual-source blending has no pipeline to draw one with"
        );
        assert!(
            !TextPlacement {
                upright: false,
                ..placement()
            }
            .keeps_subpixel(),
            "a resampled sprite smears the per-channel stripes into coloured fringes"
        );
    }

    /// A source of one glyph of a chosen format.
    struct OneGlyph(zgui_text::GlyphFormat);

    impl super::GlyphSource for OneGlyph {
        fn visit_line(
            &self,
            _paragraph: zgui_layout::fragment::ParagraphId,
            _line: u16,
            _request: super::GlyphRequest,
            visit: &mut dyn FnMut(super::GlyphRun<'_>),
        ) {
            let glyph = super::PlacedGlyph {
                resource: zgui_atlas::AtlasTile {
                    texture: zgui_atlas::TextureId {
                        kind: zgui_atlas::TextureKind::Color,
                        index: 0,
                    },
                    tile: zgui_atlas::TileId(0),
                    bounds: zgui_geom::Rect::new(
                        zgui_geom::Point::new(0, 0),
                        zgui_geom::Size::new(8, 8),
                    ),
                }
                .into(),
                bounds: Rect::new(
                    Point::new(DevicePx(0.0), DevicePx(0.0)),
                    Size::new(DevicePx(8.0), DevicePx(8.0)),
                ),
            };
            visit(super::GlyphRun {
                content: super::RunContent::Tiles(core::slice::from_ref(&glyph)),
                format: self.0,
                paint: zgui_scene::PaintSlot(0),
                synthetic_bold: 0.0,
            });
        }
    }

    /// A run of one line of the given source, over the initial style.
    fn emit_one(format: zgui_text::GlyphFormat, placement: TextPlacement) -> zgui_scene::Scene {
        let style = crate::lower::lower(&StyleDraft::initial().build(), 1.0);
        let mut scene = zgui_scene::Scene::new();
        scene.begin_frame(zgui_geom::Size::new(64, 64));
        super::emit(
            &mut scene,
            &OneGlyph(format),
            zgui_layout::fragment::ParagraphId(0),
            0,
            &style,
            super::Inherited::default(),
            placement,
        );
        scene
    }

    #[test]
    fn a_colour_glyph_takes_the_polychrome_path_and_is_not_tinted() {
        // A colour glyph is a picture. Drawing it as a coverage mask multiplies the picture by the
        // text colour, which turns every emoji into a silhouette of itself.
        let scene = emit_one(zgui_text::GlyphFormat::Color, placement());
        assert_eq!(scene.primitives.color_sprites.len(), 1);
        assert!(
            scene.primitives.mono_sprites.is_empty()
                && scene.primitives.subpixel_sprites.is_empty(),
            "a colour glyph must not also have been drawn as coverage"
        );
    }

    #[test]
    fn per_channel_coverage_is_demoted_against_a_target_that_is_not_opaque() {
        let opaque = emit_one(zgui_text::GlyphFormat::Subpixel, placement());
        assert_eq!(opaque.primitives.subpixel_sprites.len(), 1);

        let grouped = emit_one(
            zgui_text::GlyphFormat::Subpixel,
            TextPlacement {
                opaque_target: false,
                ..placement()
            },
        );
        assert!(
            grouped.primitives.subpixel_sprites.is_empty(),
            "three-channel coverage is a blend factor, and there is nothing to be a factor of"
        );
        assert_eq!(grouped.primitives.mono_sprites.len(), 1);
    }

    #[test]
    fn the_initial_style_decorates_nothing() {
        let style = DecorationStyle::of(&StyleDraft::initial().build(), 1.0);
        assert!(!style.draws_anything());
        assert_eq!(style.reach(), 0.0);
    }

    #[test]
    fn a_wave_and_a_doubled_line_are_given_a_band_and_a_stroke_is_not() {
        let wavy = DecorationStyle {
            underline: true,
            style: SceneDecorationStyle::Wavy,
            thickness: 2.0,
            ..DecorationStyle::default()
        };
        let doubled = DecorationStyle {
            style: SceneDecorationStyle::Double,
            ..wavy
        };
        let solid = DecorationStyle {
            style: SceneDecorationStyle::Solid,
            ..wavy
        };

        // A stroke is drawn in a rectangle of its own height; the two shapes evaluated across the
        // whole rectangle need one stroke either side of it or they have no shape at all.
        assert_eq!(solid.band(), 2.0);
        assert_eq!(solid.reach(), 0.0);
        assert_eq!(wavy.band(), 6.0);
        assert_eq!(doubled.band(), 6.0);
        assert!(
            wavy.band() > solid.band(),
            "handed a rectangle one stroke tall, a wave is a straight line"
        );
    }

    /// A wavy underline is drawn inside the line box it belongs to.
    ///
    /// The band is what damage would have to account for if it hung outside, and nothing tells the
    /// layout stage that an ancestor's decoration is being drawn over this line — so the band is
    /// clamped instead, and this is what says the clamp is real rather than intended.
    #[test]
    fn a_band_never_reaches_outside_the_line_box_it_decorates() {
        let placement = placement();
        let mut scene = zgui_scene::Scene::new();
        scene.begin_frame(zgui_geom::Size::new(64, 64));
        let wavy = DecorationStyle {
            underline: true,
            overline: true,
            line_through: true,
            style: SceneDecorationStyle::Wavy,
            thickness: 2.0,
            color: zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0),
        };
        let pushed = super::lines(&mut scene, &wavy, placement);
        assert_eq!(pushed, 3, "three lines were asked for and three were drawn");
        for line in &scene.primitives.decorations {
            let ink = line.ink();
            assert!(
                ink.top().0 >= placement.line.top().0
                    && ink.bottom().0 <= placement.line.bottom().0,
                "a band at {ink:?} reaches outside the line box {:?}",
                placement.line
            );
            assert!(
                ink.size.height.0 >= wavy.thickness,
                "a band thinner than its own stroke has nothing to draw: {ink:?}"
            );
        }
    }
}
