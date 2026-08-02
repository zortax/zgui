//! What a shaper is handed.

use core::ops::Range;
use std::sync::Arc;

use zgui_text_style::{ParagraphStyle, TextStyle};

use crate::brush::Brush;
use crate::map::TextMap;
use crate::paragraph::inline_box::InlineBoxGeometry;

/// One stretch of the generated string that shares a style and a brush.
#[derive(Clone, Debug, PartialEq)]
pub struct StyledRun {
    /// The byte range within the generated string.
    pub text: Range<usize>,
    /// The style, shared because the same style covers many runs in a document.
    pub style: Arc<TextStyle>,
    /// The brush the run is drawn with — an index, not a colour.
    pub brush: Brush,
}

/// One inline formatting context, ready to shape.
///
/// The string is *generated*: white space has already been collapsed, `text-transform` applied, tabs
/// expanded and the paragraph's base direction forced with a leading control. That work is the
/// caller's rather than a shaper's, for two reasons — a shaper's own collapsing is ASCII-only and
/// offers no way back to the source, and every selection, caret and hit test needs the way back,
/// which is [`TextMap`].
#[derive(Clone, Copy, Debug)]
pub struct ParagraphContent<'a> {
    /// The generated string, which the runs' ranges and every reported offset index into.
    pub text: &'a str,
    /// How to get from an offset in that string back to the source.
    pub map: &'a TextMap,
    /// The runs, in ascending order and covering the string with no gaps.
    pub runs: &'a [StyledRun],
    /// The atomic inlines packed between the words, in ascending offset order.
    pub boxes: &'a [InlineBoxGeometry],
    /// The properties the context has as a whole.
    pub paragraph: &'a ParagraphStyle,
    /// Device pixels per CSS pixel, which changes hinting and therefore the glyphs themselves.
    pub scale: f32,
}

impl ParagraphContent<'_> {
    /// Whether the runs cover the whole string in ascending order with no gaps and no overlaps.
    ///
    /// A shaper given runs that do not is entitled to any result at all, so a caller checks rather
    /// than a shaper defending against it.
    pub fn runs_are_well_formed(&self) -> bool {
        let mut next = 0;
        for run in self.runs {
            if run.text.start != next || run.text.end < run.text.start {
                return false;
            }
            next = run.text.end;
        }
        next == self.text.len()
    }
}
