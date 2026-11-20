//! Text: the glyphs of a line, the lines that decorate them, and the shadows behind both.
//!
//! # Where the glyphs come from
//!
//! This stage does not shape and does not rasterise. A fragment says *which line of which
//! paragraph* it draws, and [`GlyphSource`] is the seam that turns that into shaped runs placed
//! against the line box. Each run is recorded as one compositor glyph run, which the compositor
//! rasterises. Keeping it a seam rather than a dependency is what lets the whole paint stage be
//! exercised with no font files: a test supplies its own glyphs, and the code under test is the
//! same code.
//!
//! The seam is a visitor rather than a slice-returning method on purpose. A run's glyphs live
//! wherever the text engine put them, and a caller that had to hand out a slice would either have
//! to own one or allocate one per line, every frame.
//!
//! # When a run asks for per-channel coverage
//!
//! A run asks for subpixel coverage only when the device can draw it, the target it lands in is
//! opaque, it sits on the pixel grid and it is at most
//! [`SUBPIXEL_MAX_SIZE`](zgui_text::SUBPIXEL_MAX_SIZE) device pixels. Inside a group's own target
//! the coverage is the blend factor against a destination that is not opaque, so the run is
//! demoted whatever the device can do.

mod run;

pub(crate) use self::run::{RunDraw, flags, record};

use compositor_types::{ClipRef, Color as Premultiplied, Line, LineStyle, PaintKind, PaintRef};
use zgui_color::Color;
use zgui_css::parity::Support;
use zgui_css::values::color::{current, resolve};
use zgui_css::{ComputedStyle, register_properties};
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_layout::fragment::ParagraphId;
use zgui_scene::prim::decoration::DecorationStyle as SceneDecorationStyle;
use zgui_scene::{ClipId, ClipLink, PaintSlot, Scene, SpatialId};
use zgui_text::{FaceId, ShapedRun};

use crate::emit::record::Recorder;

register_properties! {
    text_decoration_line      => Support::Implemented("zgui-paint::emit::text"),
    text_decoration_color     => Support::Implemented("zgui-paint::emit::text"),
    text_decoration_style     => Support::Implemented("zgui-paint::emit::text"),
}

/// Where a text fragment's glyphs come from.
///
/// An implementation answers for the lines it knows about and says nothing for the rest; a line
/// with no glyphs — a blank one, or one whose paragraph has not been shaped — visits nothing, which
/// is the same answer as a line of spaces and is drawn the same way.
pub trait GlyphSource {
    /// Visits each style-uniform shaped run of one line, in the order the runs are drawn.
    ///
    /// Glyph positions are measured from the line box's top-left corner, each `y` being the
    /// distance down to the baseline.
    fn visit_line(
        &self,
        paragraph: ParagraphId,
        line: u16,
        visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
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
        _visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
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
    /// per-channel coverage whatever the device can do, and a run off the pixel grid is resampled
    /// out of the grid its stripes were measured against.
    pub fn keeps_subpixel(&self, scene: &Scene) -> bool {
        keeps_subpixel(
            scene,
            self.transform,
            self.subpixel_capable,
            self.opaque_target,
        )
    }
}

/// Whether a run drawn through `transform` keeps per-channel coverage: the device can draw it
/// (`capable`), the target it lands in is opaque (`opaque`), and the run sits on the pixel grid.
///
/// See [`TextPlacement::keeps_subpixel`] for why each of the three is needed.
pub fn keeps_subpixel(scene: &Scene, transform: SpatialId, capable: bool, opaque: bool) -> bool {
    capable && opaque && on_pixel_grid(scene, transform)
}

/// Whether the matrix `transform` resolves to moves a run by whole device pixels and does nothing
/// else.
///
/// Subpixel coverage is three side-by-side answers about thirds of one physical pixel, and it
/// survives only when the sprite lands on the pixels it was rasterised for. A turned, scaled or
/// fractionally moved run is resampled on its way to the surface, which smears the per-channel
/// stripes into coloured fringes, so it is drawn with whole-pixel coverage instead. A name the
/// frame no longer resolves answers no.
fn on_pixel_grid(scene: &Scene, transform: SpatialId) -> bool {
    scene
        .spatial
        .resolve(transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)
        .is_some_and(|affine| {
            affine.is_translation() && affine.tx.fract() == 0.0 && affine.ty.fract() == 0.0
        })
}

/// What paints a run's glyphs.
///
/// A colour is carried in the run's paint reference and takes the compositor's atlas route. A
/// paint resolved against the line box, such as a gradient across the text, takes the path route.
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
    /// The parts of nested inline boxes on the line, in tree order, whose own decorations are
    /// drawn with it.
    pub inline: &'a [crate::emit::inline::InlinePaint],
}

/// Emits one line's glyph runs and decorations, and returns how many items were recorded.
///
/// The order is CSS's: the shadows behind everything, then the glyphs, then the decoration lines
/// over them.
///
/// `decorations` is the whole list in force at this line rather than the line's own box's value,
/// because a decoration belongs to the box that declared it and is drawn across every in-flow
/// descendant of that box. A line box is generated by an anonymous inline root below the element
/// that carried the declaration, so its own style says nothing about it.
pub fn emit(
    rec: &mut Recorder<'_>,
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
        inline,
    } = inherited;
    // Resolved once for the whole line rather than per pass: a gradient across the text is
    // measured across the line box, and resolving it again per shadow would intern the same ramp
    // several times a frame.
    let brush = match text_fill {
        Some(spec) => {
            match crate::emit::vector::gradient_for_vector(
                rec.scene(),
                spec,
                placement.line,
                placement.scale,
            ) {
                Some(reference) => RunBrush::Painted(rec.slot(reference)),
                None => RunBrush::Solid,
            }
        }
        None => RunBrush::Solid,
    };
    let mut pushed = 0;
    // The mark goes on under the line's own clip; the line's glyphs go on under a tighter one that
    // stops where the content was cut. Both are decided before the first pass, so a shadow and the
    // text it shadows are cut at the same place.
    let mark = placement.ellipsis;
    let clip = cut_to_ellipsis(rec, placement);
    let drawn = Drawn { placement, clip };
    if inline.is_empty() {
        for shadow in &style.text_shadows {
            if shadow.is_invisible() {
                continue;
            }
            let offset = Size::new(DevicePx(shadow.offset_x), DevicePx(shadow.offset_y));
            pushed += runs(
                rec,
                glyphs,
                (paragraph, line),
                drawn,
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
            rec,
            glyphs,
            (paragraph, line),
            drawn,
            Painting {
                tint: style.color,
                brush,
                offset: Size::new(DevicePx(0.0), DevicePx(0.0)),
                force_mono: false,
            },
        );
    } else {
        pushed += emit_parted(rec, glyphs, (paragraph, line), style, inline, drawn, brush);
    }
    pushed += crate::emit::inline::decorations(
        rec,
        inline,
        crate::emit::inline::Lines::OverGlyphs,
        clip,
        placement.transform,
    );
    for decoration in decorations {
        pushed += lines(rec, decoration, drawn);
    }
    if let Some(mark) = mark {
        // Drawn through the line's *untightened* clip, at the boundary the cut fell on. Its own
        // box still clips it, which is what keeps a mark wider than the box from spilling out.
        let clip = rec.clip(placement.clip);
        pushed += runs(
            rec,
            glyphs,
            (mark.paragraph, 0),
            Drawn {
                placement: TextPlacement {
                    line: mark.rect(placement.line),
                    ellipsis: None,
                    ..placement
                },
                clip,
            },
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

/// Returns the clip a line's own glyphs are drawn through.
///
/// A cut line is drawn under a tighter clip than its box's, so everything past the cut goes
/// undrawn. A clip, and never a filter over the glyphs: a glyph's ink may reach past its own
/// advance, so dropping glyphs by position cuts the ink of a cluster that survived or keeps the ink
/// of a hidden one, and the specification speaks of characters.
///
/// The window is measured in the line's own coordinate system, because that is the system the line
/// box and the cut are measured in. It belongs to the line's chunk, so it moves with the chunk when
/// the line is scrolled.
fn cut_to_ellipsis(rec: &mut Recorder<'_>, placement: TextPlacement) -> ClipRef {
    let Some(mark) = placement.ellipsis else {
        return rec.clip(placement.clip);
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
    rec.local_clip(
        placement.clip,
        ClipLink::rect_in(window, placement.transform),
    )
}

/// Where one pass over a line is drawn: the placement, and the clip its items hold.
#[derive(Clone, Copy, Debug)]
struct Drawn {
    placement: TextPlacement,
    clip: ClipRef,
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

/// Emits one pass over one line's glyph runs, and returns how many were recorded.
///
/// The tint is the run's own brush unless one is given, which is what makes a shadow a second pass
/// over the same glyphs rather than a second set of them.
fn runs(
    rec: &mut Recorder<'_>,
    glyphs: &dyn GlyphSource,
    at: (ParagraphId, u16),
    drawn: Drawn,
    painting: Painting,
) -> usize {
    // A shadowed copy is coverage tinted with one colour, so it is never asked for per-channel
    // coverage: subpixel antialiasing of a blurred silhouette is fringing.
    let subpixel = drawn.placement.keeps_subpixel(rec.scene_ref()) && !painting.force_mono;
    let emitted = collect(glyphs, at);
    let mut pushed = 0;
    for run in &emitted {
        pushed += draw(rec, run, drawn, painting, subpixel);
    }
    pushed
}

/// The shaped runs of one line.
fn collect(glyphs: &dyn GlyphSource, (paragraph, line): (ParagraphId, u16)) -> Vec<Emitted> {
    // Collected rather than recorded as they are visited: a source holds its caches open for the
    // length of the visit, and recording inside it would need them and the scene at once.
    let mut emitted: Vec<Emitted> = Vec::new();
    glyphs.visit_line(paragraph, line, &mut |shaped| {
        emitted.push(Emitted {
            slot: shaped.brush,
            text_start: shaped.text_start,
            face: shaped.face,
            size: shaped.size,
            coords: shaped.coords.to_vec(),
            synthetic_bold: shaped.synthetic_bold,
            synthetic_slant: shaped.synthetic_slant,
            glyphs: shaped.glyphs.to_vec(),
        });
    });
    emitted
}

/// Records one collected run, and returns how many runs were recorded.
fn draw(
    rec: &mut Recorder<'_>,
    run: &Emitted,
    drawn: Drawn,
    painting: Painting,
    subpixel: bool,
) -> usize {
    if run.glyphs.is_empty() {
        return 0;
    }
    let paint = match painting.brush {
        RunBrush::Painted(reference) if !painting.force_mono => reference,
        _ if painting.force_mono => rec.color(painting.tint),
        _ => {
            let color = brush(rec.scene_ref(), run.slot, painting.tint);
            rec.color(color)
        }
    };
    if paint.kind == PaintKind::NONE {
        return 0;
    }
    let origin = drawn.placement.line.origin;
    let shaped = run.shaped();
    let draw = RunDraw {
        origin: [
            origin.x.0 + painting.offset.width.0,
            origin.y.0 + painting.offset.height.0,
        ],
        paint,
        clip: drawn.clip,
        space: rec.space(drawn.placement.transform),
        flags: flags(shaped.size, subpixel, painting.force_mono),
    };
    record(rec, &shaped, draw);
    1
}

/// Emits the shadows and the glyphs of a line that holds nested inline boxes, and returns how
/// many items were recorded.
///
/// Each run takes the text shadows and the shift of the innermost box that holds it, and the line's
/// own when no box does. Every shadow pass goes under every glyph, as on a line without boxes, and
/// the boxes' underlines and overlines go between the two.
fn emit_parted(
    rec: &mut Recorder<'_>,
    glyphs: &dyn GlyphSource,
    at: (ParagraphId, u16),
    style: &crate::lower::PaintStyle,
    parts: &[crate::emit::inline::InlinePaint],
    drawn: Drawn,
    text_brush: RunBrush,
) -> usize {
    let owner = |start: u32| {
        parts
            .iter()
            .rposition(|part| part.text.start <= start && start < part.text.end)
    };
    let shadows = |owner: Option<usize>| match owner {
        Some(index) => &parts[index].style.text_shadows,
        None => &style.text_shadows,
    };
    let shift = |owner: Option<usize>| owner.map_or(0.0, |index| parts[index].text.shift);
    let deepest = parts
        .iter()
        .map(|part| part.style.text_shadows.len())
        .chain([style.text_shadows.len()])
        .max()
        .unwrap_or(0);

    let mut pushed = 0;
    if deepest > 0 {
        let emitted = collect(glyphs, at);
        let owners: Vec<Option<usize>> = emitted.iter().map(|run| owner(run.text_start)).collect();
        for pass in 1..=deepest {
            for (run, owner) in emitted.iter().zip(&owners) {
                let Some(shadow) = shadows(*owner)
                    .get(pass - 1)
                    .filter(|shadow| !shadow.is_invisible())
                else {
                    continue;
                };
                // A shadowed copy is one colour whatever paints the text itself.
                let painting = Painting {
                    tint: shadow.color,
                    brush: RunBrush::Solid,
                    offset: Size::new(
                        DevicePx(shadow.offset_x),
                        DevicePx(shadow.offset_y - shift(*owner)),
                    ),
                    force_mono: true,
                };
                pushed += draw(rec, run, drawn, painting, false);
            }
        }
    }
    pushed += crate::emit::inline::decorations(
        rec,
        parts,
        crate::emit::inline::Lines::UnderGlyphs,
        drawn.clip,
        drawn.placement.transform,
    );
    let subpixel = drawn.placement.keeps_subpixel(rec.scene_ref());
    let emitted = collect(glyphs, at);
    for run in &emitted {
        let painting = Painting {
            tint: style.color,
            brush: text_brush,
            offset: Size::new(DevicePx(0.0), DevicePx(-shift(owner(run.text_start)))),
            force_mono: false,
        };
        pushed += draw(rec, run, drawn, painting, subpixel);
    }
    pushed
}

/// One visited run's shaping, held until the source's borrow of its caches has been given up.
struct Emitted {
    /// The run's brush slot.
    slot: PaintSlot,
    /// Where the run's text starts in the generated string.
    text_start: u32,
    face: FaceId,
    /// The em size, in device pixels.
    size: f32,
    /// The normalised variation coordinates, empty for the default instance.
    coords: Vec<i16>,
    /// The synthesised bold, as a fraction of the size.
    synthetic_bold: f32,
    /// The synthesised slant, in degrees.
    synthetic_slant: f32,
    /// The glyph ids and their origins from the line box's top-left corner.
    glyphs: Vec<zgui_text::ShapedGlyph>,
}

impl Emitted {
    /// Returns the run as it was visited.
    fn shaped(&self) -> ShapedRun<'_> {
        ShapedRun {
            face: self.face,
            size: self.size,
            coords: &self.coords,
            synthetic_bold: self.synthetic_bold,
            synthetic_slant: self.synthetic_slant,
            brush: self.slot,
            glyphs: &self.glyphs,
            text_start: self.text_start,
        }
    }
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
fn lines(rec: &mut Recorder<'_>, style: &DecorationStyle, drawn: Drawn) -> usize {
    if !style.draws_anything() {
        return 0;
    }
    let line = drawn.placement.line;
    let band = style.band().min(line.size.height.0.max(style.thickness));
    let space = rec.space(drawn.placement.transform);
    let mut pushed = 0;
    let mut draw = |centre: f32| {
        let top =
            (centre - band * 0.5).clamp(line.top().0, (line.bottom().0 - band).max(line.top().0));
        rec.line(Line {
            bounds: [line.origin.x.0, top, line.size.width.0, band],
            color: Premultiplied(style.color.to_premultiplied_srgb()),
            thickness: style.thickness,
            clip: drawn.clip,
            space,
            style: LineStyle(style.style as u8),
            _pad: [0; 3],
        });
        pushed += 1;
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

    use compositor_types::GlyphFlags;

    use super::{DecorationStyle, TextPlacement};
    use crate::emit::record::Recording;
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
            scale: 1.0,
            ellipsis: None,
        }
    }

    /// A scene holding one coordinate system under the viewport, at `local`.
    fn under(local: zgui_geom::Matrix4) -> (zgui_scene::Scene, SpatialId) {
        let mut scene = zgui_scene::Scene::new();
        let owner = zgui_scene::PropertyOwner::new(7).expect("a handle is never the empty word");
        let space = scene.spatial.space_of(
            SpatialId::VIEWPORT,
            owner,
            Some(zgui_scene::OwnSpace {
                local,
                anchoring: zgui_scene::Anchoring::Scrolling,
            }),
        );
        (scene, space)
    }

    #[test]
    fn subpixel_needs_a_capable_device_an_opaque_target_and_the_pixel_grid() {
        let scene = zgui_scene::Scene::new();
        assert!(placement().keeps_subpixel(&scene));
        assert!(
            !TextPlacement {
                opaque_target: false,
                ..placement()
            }
            .keeps_subpixel(&scene),
            "per-channel coverage against a transparent destination is meaningless"
        );
        assert!(
            !TextPlacement {
                subpixel_capable: false,
                ..placement()
            }
            .keeps_subpixel(&scene),
            "a device with no dual-source blending has no pipeline to draw one with"
        );
    }

    #[test]
    fn a_run_moved_by_whole_pixels_keeps_subpixel_and_any_other_matrix_loses_it() {
        use zgui_geom::Matrix4;

        let (scene, space) = under(Matrix4::translation(3.0, -4.0, 0.0));
        let moved = TextPlacement {
            transform: space,
            ..placement()
        };
        assert!(
            moved.keeps_subpixel(&scene),
            "a whole-pixel move lands the stripes on the pixels they were measured for"
        );
        for local in [
            Matrix4::translation(3.5, 0.0, 0.0),
            Matrix4::scale(1.5, 1.5, 1.0),
        ] {
            let (scene, space) = under(local);
            let placed = TextPlacement {
                transform: space,
                ..placement()
            };
            assert!(
                !placed.keeps_subpixel(&scene),
                "a resampled sprite smears the per-channel stripes into coloured fringes: {local:?}"
            );
        }
    }

    /// The glyph every test run holds, six and a half pixels in and twelve down to the baseline.
    const GLYPHS: [zgui_text::ShapedGlyph; 1] = [zgui_text::ShapedGlyph {
        glyph: 5,
        x: 6.5,
        y: 12.0,
    }];

    /// A source of one shaped run of one glyph.
    struct OneRun;

    impl super::GlyphSource for OneRun {
        fn visit_line(
            &self,
            _paragraph: zgui_layout::fragment::ParagraphId,
            _line: u16,
            visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
        ) {
            visit(zgui_text::ShapedRun {
                face: zgui_text::FaceId(1),
                size: 16.0,
                coords: &[],
                synthetic_bold: 0.0,
                synthetic_slant: 0.0,
                brush: zgui_scene::PaintSlot(0),
                glyphs: &GLYPHS,
                text_start: 0,
            });
        }
    }

    /// The items one line of [`OneRun`] records with `style`.
    fn emit_one(style: &crate::lower::PaintStyle, placement: TextPlacement) -> Recording {
        let mut scene = zgui_scene::Scene::new();
        let mut recording = Recording::new();
        let mut rec = recording.recorder(&mut scene, SpatialId::VIEWPORT, ClipId::ROOT);
        super::emit(
            &mut rec,
            &OneRun,
            zgui_layout::fragment::ParagraphId(0),
            0,
            style,
            super::Inherited::default(),
            placement,
        );
        recording
    }

    #[test]
    fn a_run_asks_for_per_channel_coverage_only_against_an_opaque_target() {
        let style = crate::lower::lower(&StyleDraft::initial().build(), 1.0);
        let opaque = emit_one(&style, placement());
        assert!(opaque.glyph_runs()[0].flags.contains(GlyphFlags::SUBPIXEL));

        let grouped = emit_one(
            &style,
            TextPlacement {
                opaque_target: false,
                ..placement()
            },
        );
        assert!(
            !grouped.glyph_runs()[0].flags.contains(GlyphFlags::SUBPIXEL),
            "three-channel coverage is a blend factor, and there is nothing to be a factor of"
        );
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
        let mut recording = Recording::new();
        let mut rec = recording.recorder(&mut scene, SpatialId::VIEWPORT, ClipId::ROOT);
        let wavy = DecorationStyle {
            underline: true,
            overline: true,
            line_through: true,
            style: SceneDecorationStyle::Wavy,
            thickness: 2.0,
            color: zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0),
        };
        let drawn = super::Drawn {
            placement,
            clip: compositor_types::ClipRef::INHERIT,
        };
        let pushed = super::lines(&mut rec, &wavy, drawn);
        assert_eq!(pushed, 3, "three lines were asked for and three were drawn");
        for line in recording.lines() {
            let [_, top, _, height] = line.bounds;
            assert!(
                top >= placement.line.top().0 && top + height <= placement.line.bottom().0,
                "a band at {:?} reaches outside the line box {:?}",
                line.bounds,
                placement.line
            );
            assert!(
                height >= wavy.thickness,
                "a band thinner than its own stroke has nothing to draw: {:?}",
                line.bounds
            );
        }
    }
}
