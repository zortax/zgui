//! Positioned glyphs from the fixed face.
//!
//! This is the half of the shaping contract painting reads, done the same way the measuring half
//! is: one glyph per character, each at the pen position of its cluster. Nothing about it is
//! typography. What it buys is that a *painting* test can
//! state where a glyph must land — the third character of the second line is at eight times two
//! plus the line's own left edge — and be wrong only if the pipeline is.

use zgui_text::{FaceId, ShapedGlyph, ShapedParagraph, ShapedRun};

use crate::shaper::cluster::MonoLayout;

/// The one face this shaper draws from.
///
/// A single handle, because there is a single face: a test that needs fallback, or two faces at
/// once, needs a real font engine.
pub const FACE: FaceId = FaceId(0);

/// The glyph index one character is drawn as.
///
/// The scalar value itself, truncated, so that a test can name the glyph it expects without
/// consulting a font table — and so that two different characters are never one glyph.
pub fn glyph_id(character: char) -> u16 {
    (character as u32 & 0xffff) as u16
}

/// Visits the runs of one line, positioned against the line box's top-left corner.
///
/// A run ends where the brush changes, which is the only style that varies across a line here: the
/// fixed face has one size and one shape. A run also ends at an inline box, and the box moves the
/// pen by its width before the character at its offset, as the line geometry places it.
pub(crate) fn visit_line(
    shaped: &ShapedParagraph<MonoLayout>,
    line: u16,
    visit: &mut dyn FnMut(ShapedRun<'_>),
) {
    let engine = &shaped.engine;
    let index = line as usize;
    let (Some((start, end)), Some(geometry)) =
        (engine.lines.get(index).copied(), engine.geometry.get(index))
    else {
        return;
    };
    let baseline = geometry.baseline.0 - geometry.top.0;
    // Device pixels, which is what a run's size is defined to be. The strut is in CSS pixels, so
    // the ratio the paragraph was shaped at converts it.
    let size = shaped.strut().font_size.0 * shaped.engine.scale;
    let mut pen = 0.0;
    let mut glyphs: Vec<ShapedGlyph> = Vec::new();
    let mut brush = None;
    let mut run_start = 0;
    for cluster in &engine.clusters[start.min(end)..end] {
        let boxed: f32 = shaped
            .boxes()
            .iter()
            .filter(|geometry| geometry.offset == cluster.offset)
            .map(|geometry| geometry.width.0)
            .sum();
        let at_box = shaped
            .boxes()
            .iter()
            .any(|geometry| geometry.offset == cluster.offset);
        if brush.is_some_and(|held| held != cluster.brush) || (brush.is_some() && at_box) {
            emit(
                visit,
                size,
                brush.take().expect("just checked"),
                &glyphs,
                run_start,
            );
            glyphs.clear();
        }
        if brush.is_none() {
            run_start = cluster.offset;
        }
        brush = Some(cluster.brush);
        pen += boxed;
        let character = shaped.text()[cluster.offset..]
            .chars()
            .next()
            .unwrap_or(' ');
        glyphs.push(ShapedGlyph {
            glyph: glyph_id(character),
            x: pen,
            y: baseline,
        });
        pen += cluster.advance.0;
    }
    if let Some(brush) = brush {
        emit(visit, size, brush, &glyphs, run_start);
    }
}

/// Hands one style-uniform stretch of a line to the visitor.
fn emit(
    visit: &mut dyn FnMut(ShapedRun<'_>),
    size: f32,
    brush: zgui_text::Brush,
    glyphs: &[ShapedGlyph],
    text_start: usize,
) {
    if glyphs.is_empty() {
        return;
    }
    visit(ShapedRun {
        face: FACE,
        size,
        coords: &[],
        synthetic_bold: 0.0,
        synthetic_slant: 0.0,
        brush,
        glyphs,
        text_start: u32::try_from(text_start).unwrap_or(u32::MAX),
    });
}
