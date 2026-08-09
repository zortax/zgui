//! Shaping one line on its own, outside the paragraph protocol.
//!
//! A paragraph is shaped, broken and measured as one object, because that is what the box tree
//! needs. Some callers need less than that and need it far more often: text laid out in cells, one
//! line at a time, in one style per run, where the wrapping is the caller's own and the same line
//! comes round again on the frame after. Shaping such a line through the paragraph protocol pays
//! for breaking it, measuring it and keying it, and hands back a result that borrows the engine.
//!
//! This is the shorter path. It shapes exactly one line, never wraps, and answers with owned runs
//! ([`ShapedRunOwned`]) that carry where each glyph came from in the string, so a caller can hold
//! them for as long as the line is unchanged.

use zgui_geom::CssPx;
use zgui_interned::Ident;
use zgui_text::{
    FaceId, FaceMetrics, FontSource, SYNTHETIC_BOLD_RATIO, ShapedGlyph, ShapedRunOwned,
};
use zgui_text_style::variant::FontVariantLigatures;
use zgui_text_style::{
    FamilyName, FontFamilyList, FontSlant, FontVariant, LengthPercent, TextStyle, WrapMode,
};

use crate::shape::brush::SlotBrush;
use crate::shape::style::LoweredStyle;
use crate::system::FontSystem;

/// One line of one uniform style, asked for in the terms a caller laying out cells has.
///
/// The families are tried in order, and the first that can draw a character wins it; a character
/// none of them covers falls back the way any other text does. Everything is in device pixels,
/// which is what the shaped result carries.
#[derive(Clone, Copy, Debug)]
pub struct LineRequest<'a> {
    /// The families to try, in order.
    pub families: &'a [Ident],
    /// The CSS weight, from 100 to 1000.
    pub weight: u16,
    /// Whether to ask for an italic face.
    pub italic: bool,
    /// The size to shape at, in device pixels.
    pub size_device_px: f32,
    /// Extra space added after every glyph, in device pixels.
    pub letter_spacing: f32,
    /// Whether the face's ligatures and contextual alternates are applied.
    ///
    /// Off turns off all four ligature groups, which is what text laid out in fixed cells needs: a
    /// ligature draws two characters as one glyph and takes one cell doing it.
    pub ligatures: bool,
}

/// What one request's line is measured by, in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedLineMetrics {
    /// The face the request resolves to, which is the face the first run of its text is set in.
    pub face: FaceId,
    /// That face's metrics at the requested size.
    pub metrics: FaceMetrics,
    /// The advance of one cell: the face's zero advance, or the documented fallback for a face
    /// that has no digit zero.
    pub cell_advance: CssPx,
}

/// The style a request lowers to.
///
/// Shared by shaping and by measuring, so the face the metrics describe is the face the glyphs are
/// drawn from.
pub(crate) fn text_style(request: &LineRequest<'_>) -> TextStyle {
    TextStyle {
        family: FontFamilyList::from_iter(
            request
                .families
                .iter()
                .copied()
                .map(FamilyName::Named)
                .collect::<Vec<_>>(),
        ),
        size: CssPx(request.size_device_px),
        weight: f32::from(request.weight),
        slant: if request.italic {
            FontSlant::Italic
        } else {
            FontSlant::Upright
        },
        letter_spacing: LengthPercent::length(CssPx(request.letter_spacing)),
        wrap_mode: WrapMode::NoWrap,
        variant: FontVariant {
            ligatures: if request.ligatures {
                FontVariantLigatures::NORMAL
            } else {
                FontVariantLigatures::none()
            },
            ..FontVariant::default()
        },
        ..TextStyle::initial()
    }
}

/// Shapes one line, and reads its runs out owned.
///
/// The string is shaped exactly as it was given: no directional mark is prefixed, so the base
/// direction is detected from the content and every byte offset reported is an offset into `text`
/// itself. A caller that must force the base direction writes its own mark, and then that mark is
/// part of its string and is counted by the offsets like any other character.
///
/// `text` is one line. A mandatory break in it is the caller's own error; only the first line of
/// the result is read.
pub(crate) fn shape(
    text: &str,
    request: &LineRequest<'_>,
    fonts: &FontSystem,
    context: &mut parley::FontContext,
    scratch: &mut parley::LayoutContext<SlotBrush>,
) -> Vec<ShapedRunOwned> {
    if text.is_empty() {
        return Vec::new();
    }
    // The brush is a placeholder: it never leaves this function, because an owned run takes its
    // slot from whoever draws it.
    let lowered = LoweredStyle::of(&text_style(request), zgui_scene::PaintSlot(0), CssPx::ZERO);
    // A scale of one, because the size is already in device pixels.
    let mut builder = scratch.ranged_builder(context, text, 1.0, true);
    lowered.push_default(&mut builder);
    let mut layout: parley::Layout<SlotBrush> = builder.build(text);
    layout.break_all_lines(None);
    debug_assert!(
        layout.len() <= 1,
        "a line request holds one line, and wrapping is off"
    );

    let Some(line) = layout.get(0) else {
        return Vec::new();
    };
    let metrics = *line.metrics();
    let left = metrics.offset + metrics.inline_min_coord;
    let top = metrics.block_min_coord;

    let mut runs = Vec::new();
    for item in line.items() {
        let parley::PositionedLayoutItem::GlyphRun(run) = item else {
            continue;
        };
        let glyphs: Vec<ShapedGlyph> = run
            .positioned_glyphs()
            .map(|glyph| ShapedGlyph {
                glyph: glyph.id as u16,
                x: glyph.x - left,
                y: glyph.y - top,
            })
            .collect();
        if glyphs.is_empty() {
            continue;
        }
        let clusters = cluster_bytes(&run, glyphs.len());
        let text_run = run.run();
        let face = fonts.face_for(text_run.font());
        let synthesis = text_run.synthesis();
        runs.push(ShapedRunOwned {
            face,
            size: text_run.font_size(),
            synthetic_bold: if synthesis.embolden() {
                SYNTHETIC_BOLD_RATIO
            } else {
                0.0
            },
            synthetic_slant: synthesis.skew().unwrap_or(0.0),
            has_color: fonts.face(face).is_some_and(|record| record.has_color),
            glyphs,
            clusters,
        });
    }
    runs
}

/// The byte each glyph's cluster starts at, one entry per glyph.
///
/// Read from the run's visual clusters, which is the order the positioned glyphs are produced in,
/// so the two lists line up without being matched up. A cluster drawn as several glyphs repeats its
/// byte; a cluster drawn as one glyph — a ligature — reports it once.
fn cluster_bytes(run: &parley::GlyphRun<'_, SlotBrush>, glyphs: usize) -> Vec<u32> {
    let mut bytes = Vec::with_capacity(glyphs);
    for cluster in run.run().visual_clusters() {
        let start = cluster.text_range().start as u32;
        for _ in cluster.glyphs() {
            bytes.push(start);
        }
    }
    debug_assert_eq!(
        bytes.len(),
        glyphs,
        "one cluster byte per positioned glyph of the run"
    );
    // A run whose clusters and glyphs disagree is an engine change rather than a caller error, and
    // a short list would panic whoever indexes it. Padding with the last byte keeps the invariant
    // the type documents.
    let last = bytes.last().copied().unwrap_or(0);
    bytes.resize(glyphs, last);
    bytes
}
