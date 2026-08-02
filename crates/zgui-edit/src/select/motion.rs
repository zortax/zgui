//! Moving the caret over the text.

use crate::select::{Selection, grapheme, word};
use crate::text::EditText;

/// How far one movement goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Granularity {
    /// One grapheme: what an arrow key moves.
    Grapheme,
    /// One word: what a control-arrow moves.
    Word,
    /// To the edge of the paragraph the caret is in: what home and end move.
    Paragraph,
    /// To the edge of the whole text.
    Document,
}

/// One movement of the caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Motion {
    /// How far.
    pub granularity: Granularity,
    /// Whether it goes towards the end of the text.
    pub forwards: bool,
    /// Whether the anchor stays where it is, extending the selection.
    pub extend: bool,
}

impl Motion {
    /// A movement by one unit.
    pub const fn new(granularity: Granularity, forwards: bool, extend: bool) -> Self {
        Self {
            granularity,
            forwards,
            extend,
        }
    }
}

/// Where a selection ends up after a movement.
///
/// A movement that is not extending and starts from a selection collapses it towards the direction
/// of travel rather than moving from the focus — pressing right with three words selected puts the
/// caret after them, which is what every text field does and what a naive "move the focus" does
/// not.
///
/// ```
/// use zgui_edit::select::{Granularity, Motion, Selection, motion};
/// use zgui_edit::text::EditText;
///
/// let text = EditText::of("one two");
/// let selected = Selection::new(0, 3);
/// let collapsed = motion::apply(&text, selected, Motion::new(Granularity::Grapheme, true, false));
/// assert_eq!(collapsed, Selection::caret(3), "the selection collapses, the caret does not step");
/// ```
pub fn apply(text: &EditText, selection: Selection, motion: Motion) -> Selection {
    let content = text.text();
    if !motion.extend && !selection.is_caret() && motion.granularity == Granularity::Grapheme {
        return if motion.forwards {
            selection.collapsed_to_end()
        } else {
            selection.collapsed_to_start()
        };
    }
    let from = selection.focus.min(content.len());
    let to = match (motion.granularity, motion.forwards) {
        (Granularity::Grapheme, true) => grapheme::next(&content, from),
        (Granularity::Grapheme, false) => grapheme::previous(&content, from),
        (Granularity::Word, true) => word::next(&content, from),
        (Granularity::Word, false) => word::previous(&content, from),
        (Granularity::Paragraph, true) => {
            let position = text.position_of(from);
            text.start_of(position.paragraph)
                + text.paragraph(position.paragraph).map_or(0, str::len)
        }
        (Granularity::Paragraph, false) => text.start_of(text.position_of(from).paragraph),
        (Granularity::Document, true) => content.len(),
        (Granularity::Document, false) => 0,
    };
    selection.moved_to(to, motion.extend)
}

#[cfg(test)]
mod tests {
    use super::{Granularity, Motion, apply};
    use crate::select::Selection;
    use crate::text::EditText;

    /// The text every case here moves over.
    fn text() -> EditText {
        EditText::of("one two\nthree four")
    }

    #[test]
    fn a_grapheme_step_crosses_the_break_between_two_paragraphs() {
        let text = text();
        let at_end_of_first = Selection::caret(7);
        let moved = apply(
            &text,
            at_end_of_first,
            Motion::new(Granularity::Grapheme, true, false),
        );
        assert_eq!(moved.focus, 8, "the break is one step and one byte");
    }

    #[test]
    fn extending_keeps_the_anchor_where_it_was() {
        let text = text();
        let extended = apply(
            &text,
            Selection::caret(4),
            Motion::new(Granularity::Word, true, true),
        );
        assert_eq!(extended.anchor, 4);
        assert_eq!(extended.focus, 8);
        assert_eq!(extended.range(), 4..8);
    }

    #[test]
    fn a_paragraph_edge_is_the_paragraph_the_caret_is_in_and_not_the_document() {
        let text = text();
        let home = apply(
            &text,
            Selection::caret(12),
            Motion::new(Granularity::Paragraph, false, false),
        );
        assert_eq!(home.focus, 8, "the start of the second paragraph");
        let end = apply(
            &text,
            Selection::caret(12),
            Motion::new(Granularity::Paragraph, true, false),
        );
        assert_eq!(end.focus, 18, "and its end, not the end of the text");
    }

    #[test]
    fn the_document_edges_are_the_whole_text() {
        let text = text();
        assert_eq!(
            apply(
                &text,
                Selection::caret(3),
                Motion::new(Granularity::Document, true, false)
            )
            .focus,
            text.len()
        );
        assert_eq!(
            apply(
                &text,
                Selection::caret(3),
                Motion::new(Granularity::Document, false, false)
            )
            .focus,
            0
        );
    }

    #[test]
    fn a_word_step_from_a_selection_still_moves_the_focus() {
        // Only a grapheme step collapses: control-right with a selection extends past it the way
        // every field does, rather than stopping at its edge.
        let text = text();
        let moved = apply(
            &text,
            Selection::new(0, 3),
            Motion::new(Granularity::Word, true, false),
        );
        assert_eq!(moved.focus, 4);
        assert!(moved.is_caret());
    }
}
