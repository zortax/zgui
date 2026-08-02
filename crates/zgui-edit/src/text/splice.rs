//! What one replacement did to the paragraph list.

use core::ops::Range;

/// Which paragraphs a replacement removed, and how many it put in their place.
///
/// This is the whole reason the buffer is a list of paragraphs rather than one string. A shaper
/// has no incremental mode: shaping is per paragraph and one character re-shapes whichever
/// paragraphs it touched. A typed letter reports one paragraph removed and one inserted, so
/// exactly one paragraph's content changed and every other cached shape is still valid; pressing
/// return reports one removed and two inserted; pasting a page reports one removed and as many
/// inserted as it had lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Splice {
    /// The paragraphs that are no longer there, by their index before the replacement.
    pub removed: Range<usize>,
    /// How many paragraphs took their place, starting at `removed.start`.
    pub inserted: usize,
}

impl Splice {
    /// The indices the new paragraphs occupy after the replacement.
    pub fn inserted_range(&self) -> Range<usize> {
        self.removed.start..self.removed.start + self.inserted
    }

    /// Whether the paragraph *count* is unchanged, which is the ordinary case of typing into one.
    pub fn is_in_place(&self) -> bool {
        self.removed.len() == self.inserted
    }
}
