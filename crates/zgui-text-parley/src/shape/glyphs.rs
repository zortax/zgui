//! Reading positioned glyphs out of a broken layout.
//!
//! # Why the positions are shifted
//!
//! The engine positions every glyph relative to the *paragraph's* origin, and folds the line's
//! alignment offset and its inline minimum coordinate into that position. A line box, on the other
//! hand, starts exactly where those two put it. Subtracting them therefore moves the glyphs into
//! the line box's own space, which is the space everything downstream places a line in — and it is
//! the only conversion here, because the engine is already working in device pixels.

use zgui_text::{FontSource, SYNTHETIC_BOLD_RATIO, ShapedGlyph, ShapedRun};

use crate::shape::brush::SlotBrush;
use crate::shape::engine::ShapedLayout;
use crate::system::FontSystem;

/// Visits the style-uniform runs of one line, positioned against the line box's top-left corner.
pub(crate) fn visit_line(
    fonts: &FontSystem,
    shaped: &ShapedLayout,
    line: u16,
    visit: &mut dyn FnMut(ShapedRun<'_>),
) {
    let Some(line) = shaped.layout.get(line as usize) else {
        return;
    };
    let metrics = *line.metrics();
    let left = metrics.offset + metrics.inline_min_coord;
    let top = metrics.block_min_coord;

    // One buffer for the whole line rather than one per run: a line of prose is a handful of runs
    // and every one of them would otherwise allocate.
    let mut glyphs: Vec<ShapedGlyph> = Vec::new();
    for item in line.items() {
        let parley::PositionedLayoutItem::GlyphRun(run) = item else {
            continue;
        };
        glyphs.clear();
        glyphs.extend(run.positioned_glyphs().map(|glyph| ShapedGlyph {
            glyph: glyph.id as u16,
            x: glyph.x - left,
            y: glyph.y - top,
        }));
        if glyphs.is_empty() {
            continue;
        }
        visit(described(fonts, &run, &glyphs));
    }
}

/// One engine run, described the way a rasteriser needs it.
fn described<'a>(
    fonts: &FontSystem,
    run: &parley::GlyphRun<'_, SlotBrush>,
    glyphs: &'a [ShapedGlyph],
) -> ShapedRun<'a> {
    let text = run.run();
    let face = fonts.face_for(text.font());
    let synthesis = text.synthesis();
    ShapedRun {
        face,
        size: text.font_size(),
        // The engine reports *whether* to embolden; how much is a rasterisation decision, and it
        // is the same one wherever a synthesised bold is drawn.
        synthetic_bold: if synthesis.embolden() {
            SYNTHETIC_BOLD_RATIO
        } else {
            0.0
        },
        synthetic_slant: synthesis.skew().unwrap_or(0.0),
        has_color: fonts.face(face).is_some_and(|record| record.has_color),
        brush: run.style().brush.0,
        glyphs,
    }
}
