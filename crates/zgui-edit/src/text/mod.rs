//! The text being edited, held as the paragraphs a shaper works in.
//!
//! One string with the newlines left in it would be simpler and would cost a full re-shape per
//! keystroke, because a shaper shapes a paragraph at a time and has no incremental mode. So the
//! buffer *is* the paragraph list, a document offset is resolved through it, and every replacement
//! reports which paragraphs it touched.

pub mod position;
pub mod splice;

use core::ops::Range;

pub use crate::text::position::Position;
pub use crate::text::splice::Splice;

/// The character a paragraph ends at, and the only one the buffer never stores.
const BREAK: char = '\n';

/// The text being edited, split at hard line breaks.
///
/// There is always at least one paragraph, so an empty buffer is one empty paragraph rather than
/// none — which is what makes "the caret is at offset 0" expressible in an empty field.
///
/// Document offsets count every paragraph's bytes plus one byte for each break between them, so
/// they are offsets into the string [`text`](EditText::text) returns and the offsets every
/// selection, accessibility range and event payload is expressed in.
///
/// ```
/// use zgui_edit::text::{EditText, Position};
///
/// let mut text = EditText::of("one\ntwo");
/// assert_eq!(text.paragraphs().len(), 2);
/// assert_eq!(text.len(), 7);
/// assert_eq!(text.position_of(4), Position::new(1, 0));
///
/// // Typing into the second paragraph leaves the first one's content untouched.
/// let splice = text.replace(4..4, "X");
/// assert_eq!(splice.removed, 1..2);
/// assert_eq!(splice.inserted, 1);
/// assert_eq!(text.text(), "one\nXtwo");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditText {
    /// The paragraphs, none of which holds a line break.
    paragraphs: Vec<String>,
}

impl Default for EditText {
    fn default() -> Self {
        Self::new()
    }
}

impl EditText {
    /// An empty buffer, which is one empty paragraph.
    pub fn new() -> Self {
        Self {
            paragraphs: vec![String::new()],
        }
    }

    /// The buffer holding `text`, split at its line breaks.
    pub fn of(text: &str) -> Self {
        Self {
            paragraphs: text.split(BREAK).map(str::to_owned).collect(),
        }
    }

    /// The paragraphs, in order.
    pub fn paragraphs(&self) -> &[String] {
        &self.paragraphs
    }

    /// One paragraph's text.
    pub fn paragraph(&self, index: usize) -> Option<&str> {
        self.paragraphs.get(index).map(String::as_str)
    }

    /// The whole text, breaks included.
    pub fn text(&self) -> String {
        self.paragraphs.join("\n")
    }

    /// How many bytes the whole text occupies, breaks included.
    pub fn len(&self) -> usize {
        self.paragraphs
            .iter()
            .map(|paragraph| paragraph.len() + 1)
            .sum::<usize>()
            - 1
    }

    /// Whether there is nothing to edit.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The document offset a paragraph starts at.
    ///
    /// An index past the last paragraph answers with the end of the text, which is where an
    /// insertion after every paragraph goes.
    pub fn start_of(&self, paragraph: usize) -> usize {
        self.paragraphs
            .iter()
            .take(paragraph)
            .map(|held| held.len() + 1)
            .sum()
    }

    /// Where a document offset falls.
    ///
    /// An offset past the end resolves to the end of the last paragraph rather than being refused,
    /// because every caller of this holds an offset that some edit may already have shortened.
    pub fn position_of(&self, offset: usize) -> Position {
        let mut start = 0;
        for (index, paragraph) in self.paragraphs.iter().enumerate() {
            if offset <= start + paragraph.len() {
                return Position::new(index, offset - start);
            }
            start += paragraph.len() + 1;
        }
        let last = self.paragraphs.len().saturating_sub(1);
        Position::new(last, self.paragraphs[last].len())
    }

    /// The document offset a position names.
    pub fn offset_of(&self, position: Position) -> usize {
        let paragraph = position
            .paragraph
            .min(self.paragraphs.len().saturating_sub(1));
        self.start_of(paragraph) + position.offset.min(self.paragraphs[paragraph].len())
    }

    /// The text in a range of document offsets.
    ///
    /// Read out of the paragraphs the range actually covers, never out of the whole text: this is
    /// on the path of every keystroke — it is what an undo entry records as removed — and joining
    /// the document to copy one character out of it makes typing cost the length of the document.
    pub fn slice(&self, range: Range<usize>) -> String {
        let first = self.position_of(self.clamp(range.start));
        let last = self.position_of(self.clamp(range.end.max(range.start)));
        if first.paragraph == last.paragraph {
            return self.paragraphs[first.paragraph][first.offset..last.offset].to_owned();
        }
        let mut taken = self.paragraphs[first.paragraph][first.offset..].to_owned();
        for paragraph in &self.paragraphs[first.paragraph + 1..last.paragraph] {
            taken.push(BREAK);
            taken.push_str(paragraph);
        }
        taken.push(BREAK);
        taken.push_str(&self.paragraphs[last.paragraph][..last.offset]);
        taken
    }

    /// The nearest offset at or before `offset` that is inside the text and on a character
    /// boundary.
    ///
    /// Resolved inside the one paragraph the offset falls in, for the same reason
    /// [`slice`](EditText::slice) is: the answer never depends on any other paragraph, and every
    /// edit asks for it.
    pub fn clamp(&self, offset: usize) -> usize {
        let position = self.position_of(offset);
        let paragraph = &self.paragraphs[position.paragraph];
        self.start_of(position.paragraph) + clamp_boundary(paragraph, position.offset)
    }

    /// Replaces a range of document offsets, reporting which paragraphs changed.
    ///
    /// The range is clamped to the text and to character boundaries, so an offset left over from
    /// before an earlier edit shortens the text rather than splitting a character or panicking.
    pub fn replace(&mut self, range: Range<usize>, with: &str) -> Splice {
        let start = self.clamp(range.start);
        let end = self.clamp(range.end.max(start));
        let first = self.position_of(start);
        let last = self.position_of(end);

        let head = self.paragraphs[first.paragraph][..first.offset].to_owned();
        let tail = self.paragraphs[last.paragraph][last.offset..].to_owned();
        let replacement = format!("{head}{with}{tail}");
        let inserted: Vec<String> = replacement.split(BREAK).map(str::to_owned).collect();

        let removed = first.paragraph..last.paragraph + 1;
        let count = inserted.len();
        self.paragraphs.splice(removed.clone(), inserted);
        Splice {
            removed,
            inserted: count,
        }
    }
}

/// The nearest character boundary at or before `offset`, never past the end of `text`.
fn clamp_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::{EditText, Position};

    #[test]
    fn an_empty_buffer_is_one_empty_paragraph_and_not_none() {
        let text = EditText::new();
        assert_eq!(text.paragraphs().len(), 1);
        assert_eq!(text.len(), 0);
        assert!(text.is_empty());
        assert_eq!(text.position_of(0), Position::new(0, 0));
    }

    #[test]
    fn a_break_is_one_byte_between_two_paragraphs() {
        let text = EditText::of("ab\ncd");
        assert_eq!(text.len(), 5);
        assert_eq!(text.start_of(1), 3);
        assert_eq!(text.position_of(2), Position::new(0, 2), "end of the first");
        assert_eq!(
            text.position_of(3),
            Position::new(1, 0),
            "start of the second"
        );
        assert_eq!(text.slice(2..4), "\nc");
    }

    #[test]
    fn typing_into_one_paragraph_replaces_exactly_that_paragraph() {
        let mut text = EditText::of("one\ntwo\nthree");
        let splice = text.replace(5..5, "W");
        assert_eq!(splice.removed, 1..2);
        assert!(splice.is_in_place());
        assert_eq!(text.paragraphs(), ["one", "tWwo", "three"]);
    }

    #[test]
    fn a_break_inserted_in_the_middle_splits_one_paragraph_into_two() {
        let mut text = EditText::of("abcd");
        let splice = text.replace(2..2, "\n");
        assert_eq!(splice.removed, 0..1);
        assert_eq!(splice.inserted, 2);
        assert_eq!(text.paragraphs(), ["ab", "cd"]);
    }

    #[test]
    fn a_deletion_across_a_break_joins_the_two_paragraphs() {
        let mut text = EditText::of("ab\ncd");
        let splice = text.replace(2..3, "");
        assert_eq!(splice.removed, 0..2);
        assert_eq!(splice.inserted, 1);
        assert_eq!(text.paragraphs(), ["abcd"]);
    }

    #[test]
    fn an_offset_inside_a_character_is_clamped_rather_than_splitting_it() {
        let mut text = EditText::of("é");
        assert_eq!(text.clamp(1), 0);
        text.replace(1..2, "x");
        assert_eq!(text.text(), "x", "the whole character went, not half of it");
    }

    #[test]
    fn a_slice_spanning_several_paragraphs_reads_the_breaks_between_them_back() {
        // Read out of the paragraphs rather than out of the joined text, so the paragraphs in the
        // middle and the breaks on both sides of them are the part that has to be got right.
        let text = EditText::of("one\ntwo\nthree\nfour");
        assert_eq!(text.slice(1..16), "ne\ntwo\nthree\nfo");
        assert_eq!(text.slice(3..4), "\n", "a break on its own");
        assert_eq!(text.slice(0..text.len()), text.text());
        assert_eq!(text.slice(5..5), "");
    }

    #[test]
    fn clamping_and_slicing_never_split_a_character_in_a_later_paragraph() {
        let text = EditText::of("ab\ncé\ndé");
        // The two bytes of é in the second paragraph, which is one character starting here.
        let accented = text.start_of(1) + 1;
        assert_eq!(
            text.clamp(accented + 1),
            accented,
            "an offset inside é came back inside it"
        );
        assert_eq!(text.slice(accented..accented + 1), "", "half a character");
        assert_eq!(text.slice(accented..accented + 2), "é");
        assert_eq!(text.clamp(999), text.len());
    }

    #[test]
    fn an_offset_past_the_end_lands_at_the_end() {
        let text = EditText::of("ab\ncd");
        assert_eq!(text.position_of(99), Position::new(1, 2));
        assert_eq!(text.offset_of(Position::new(9, 9)), 5);
    }
}
