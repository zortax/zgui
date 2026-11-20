//! Reading positioned glyphs out of a broken layout.
//!
//! # Why the positions are shifted
//!
//! The engine positions every glyph relative to the *paragraph's* origin, and folds the line's
//! alignment offset and its inline minimum coordinate into that position. A line box, on the other
//! hand, starts exactly where those two put it. Subtracting them therefore moves the glyphs into
//! the line box's own space, which is the space everything downstream places a line in — and it is
//! the only conversion here, because the engine is already working in device pixels.

use zgui_text::{Brush, FontSource, SYNTHETIC_BOLD_RATIO, ShapedGlyph, ShapedRun};

use crate::shape::brush::SlotBrush;
use crate::shape::engine::{Form, ShapedLayout};
use crate::system::FontSystem;

/// Visits the style-uniform runs of one line, positioned against the line box's top-left corner.
pub(crate) fn visit_line(
    fonts: &FontSystem,
    shaped: &ShapedLayout,
    line: u16,
    visit: &mut dyn FnMut(ShapedRun<'_>),
) {
    let layout = match &shaped.form {
        Form::Live(live) => &live.layout,
        Form::Frozen(frozen) => return frozen.visit_line(fonts, line, visit),
    };
    let Some(line) = layout.get(line as usize) else {
        return;
    };
    let metrics = *line.metrics();
    let left = metrics.offset + metrics.inline_min_coord;
    let top = crate::shape::lines::top(&metrics);

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
        visit(described(fonts, &run, &glyphs, shaped.prefix));
    }
}

/// One engine run, described the way a rasteriser needs it.
///
/// `prefix` is how many bytes of directional control lead the engine's string; the run's text
/// offset is reported in the caller's string, which does not hold them.
fn described<'a>(
    fonts: &FontSystem,
    run: &parley::GlyphRun<'_, SlotBrush>,
    glyphs: &'a [ShapedGlyph],
    prefix: usize,
) -> ShapedRun<'a> {
    let text = run.run();
    let text_start = text.text_range().start.saturating_sub(prefix);
    let synthesis = text.synthesis();
    run_of(
        fonts,
        text.font(),
        text.normalized_coords(),
        text.font_size(),
        synthesis.embolden(),
        synthesis.skew().unwrap_or(0.0),
        run.style().brush.0,
        glyphs,
        u32::try_from(text_start).unwrap_or(u32::MAX),
    )
}

/// One run of glyphs in one face, described the way a rasteriser needs it.
///
/// `embolden` and `skew` are the engine's synthesis. `text_start` is in the caller's string.
#[expect(
    clippy::too_many_arguments,
    reason = "the fields of one engine run, read once"
)]
pub(crate) fn run_of<'a>(
    fonts: &FontSystem,
    font: &parley::FontData,
    coords: &[i16],
    size: f32,
    embolden: bool,
    skew: f32,
    brush: Brush,
    glyphs: &'a [ShapedGlyph],
    text_start: u32,
) -> ShapedRun<'a> {
    let face = fonts.face_for(font);
    ShapedRun {
        face,
        size,
        instance: fonts.variation_instance(coords),
        // The engine reports *whether* to embolden; how much is a rasterisation decision, and it
        // is the same one wherever a synthesised bold is drawn.
        synthetic_bold: if embolden { SYNTHETIC_BOLD_RATIO } else { 0.0 },
        synthetic_slant: skew,
        has_color: fonts.face(face).is_some_and(|record| record.has_color),
        brush,
        glyphs,
        text_start,
    }
}
