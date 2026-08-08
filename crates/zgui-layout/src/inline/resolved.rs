//! What one inline formatting context came out at, kept for everything that reads layout later.
//!
//! A measure call is asked many times at many widths and only the last one is the answer. What is
//! kept here is that answer: the lines, which paragraph's glyphs they are lines of, and where the
//! atomic inlines landed. Painting, hit testing and accessibility all read this rather than
//! re-deriving it, because re-deriving it would mean breaking the paragraph again and getting a
//! second opinion.

use zgui_dom::side::BoxKey;
use zgui_text::{ParagraphKey, TextMap};

use crate::fragment::ParagraphId;
use crate::inline::ellipsis::EllipsisSource;
use crate::inline::lines::LineBox;

/// Where one atomic inline ended up inside its context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The box.
    pub box_: BoxKey,
    /// Its real top-left corner, relative to the context's content box.
    pub origin: (f32, f32),
    /// Which line it landed on.
    pub line: usize,
}

/// One inline formatting context's finished layout.
#[derive(Clone, Debug)]
pub struct InlineResolution {
    /// The paragraph the lines are lines of.
    pub paragraph: ParagraphId,
    /// The key its shaped glyphs are held under.
    pub key: ParagraphKey,
    /// The line boxes, in visual order, relative to the context's content box.
    pub lines: Vec<LineBox>,
    /// Where each atomic inline ended up.
    pub placements: Vec<Placement>,
    /// Whether the base direction came out right-to-left.
    pub is_rtl: bool,
    /// How to get from an offset in the string the shaper was handed back to the document.
    ///
    /// The lines, the clusters and every hit test are expressed in the *generated* string — white
    /// space collapsed, tabs expanded, a direction control prefixed — which the document never
    /// contained. A caret, a selection and an accessibility range are all expressed in the text the
    /// document does hold, so nothing can cross between the two without this.
    pub map: TextMap,
    /// The box each text run of that string came from, indexed by the run number the map reports.
    pub sources: Vec<BoxKey>,
    /// What the lines that did not fit are cut off with.
    ///
    /// Empty unless some line overflowed *and* the box asked for a mark, so a context whose text
    /// fits carries nothing and shapes nothing. The marks are shaped paragraphs of their own, which
    /// is why they are held here: their glyphs have to stay alive for exactly as long as the lines
    /// that name them.
    pub ellipsis: EllipsisSource,
}

impl InlineResolution {
    /// Every paragraph this resolution names: the lines' own, and the marks that cut them.
    ///
    /// What the store retains and releases. A mark is a shaped paragraph like any other and is
    /// evicted like any other, so a resolution that named one without holding it would be a line
    /// drawn with an ellipsis that had been thrown away.
    pub fn paragraphs(&self) -> impl Iterator<Item = ParagraphId> + '_ {
        core::iter::once(self.paragraph).chain(self.ellipsis.paragraphs())
    }

    /// The baseline a parent aligns this context's first line against.
    pub fn first_baseline(&self) -> Option<f32> {
        self.lines.first().map(LineBox::baseline)
    }

    /// The baseline a parent aligns its last line against, which is what an atomic inline in
    /// normal flow is aligned by.
    pub fn last_baseline(&self) -> Option<f32> {
        self.lines.last().map(LineBox::baseline)
    }
}
