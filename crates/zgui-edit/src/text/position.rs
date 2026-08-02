//! Where an offset falls, in paragraphs.

/// A document offset, resolved to the paragraph it lands in.
///
/// The offset inside the paragraph counts bytes of that paragraph's own text, so it is never past
/// the paragraph's end: a document offset that names the break between two paragraphs resolves to
/// the end of the earlier one, which is where a caret at that offset is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    /// Which paragraph, counting from zero.
    pub paragraph: usize,
    /// The byte offset inside that paragraph's text.
    pub offset: usize,
}

impl Position {
    /// A position in a paragraph.
    pub const fn new(paragraph: usize, offset: usize) -> Self {
        Self { paragraph, offset }
    }
}
