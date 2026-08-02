//! One verbatim stretch, and the position it came from.

use core::ops::Range;

/// One stretch of generated text that is a byte-for-byte copy of a stretch of source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    /// The byte range in the generated string.
    pub generated: Range<usize>,
    /// Which source run the bytes came from.
    pub run: usize,
    /// The byte offset inside that run's own text where the stretch starts.
    pub offset: usize,
}

/// A position in the source text.
///
/// A run rather than a document node, because generation happens over the runs an inline formatting
/// context was flattened into, and a consumer that needs a node maps a run to one itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourcePos {
    /// Which run.
    pub run: usize,
    /// The byte offset inside that run's text.
    pub offset: usize,
}
