//! What is being composed, and where.

use core::ops::Range;

use crate::select::Selection;

/// A composition in progress.
///
/// The range is where the provisional text sits in the document right now, and it is rewritten by
/// every preedit: an input method replaces what it previously offered rather than appending to it.
///
/// [`restore`](Composition::restore) is what the composition started from — the text that was
/// there and the selection that was in it — and it is kept for two reasons that are the same
/// reason: an abandoned composition puts back exactly what it displaced, and a committed one is
/// recorded as a single undoable change from that starting point rather than as one change per
/// keystroke the input method consumed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composition {
    /// Where the provisional text sits now.
    pub range: Range<usize>,
    /// The text that was replaced when the composition started.
    pub restore: String,
    /// Where the replaced text started.
    pub restore_at: usize,
    /// The selection before the composition started.
    pub restore_selection: Selection,
}

impl Composition {
    /// A composition displacing `restore`, which starts at `at`.
    ///
    /// The range starts as the range being displaced, so the first preedit replaces it rather than
    /// being inserted beside it: a composition started over a selection puts its provisional text
    /// where that selection was.
    pub fn started(at: usize, restore: String, selection: Selection) -> Self {
        Self {
            range: at..at + restore.len(),
            restore,
            restore_at: at,
            restore_selection: selection,
        }
    }

    /// The range the composition started from, which is what an abandoned one puts back.
    pub fn restore_range(&self) -> Range<usize> {
        self.restore_at..self.restore_at + self.restore.len()
    }

    /// Where the caret goes for a preedit cursor reported inside the provisional text.
    ///
    /// The offsets an input method reports are into the provisional text alone, so they are
    /// shifted into the document and clamped to the composition — a candidate window that reports
    /// a cursor past the text it just sent must not move the caret out of the composition.
    /// Reported as no cursor at all, the caret goes to the end of the provisional text, which is
    /// where every platform's own fallback puts it.
    pub fn caret_for(&self, cursor: Option<Range<usize>>) -> Selection {
        match cursor {
            Some(cursor) => {
                let start = (self.range.start + cursor.start).min(self.range.end);
                let end = (self.range.start + cursor.end).min(self.range.end);
                Selection::new(start.min(end), end.max(start))
            }
            None => Selection::caret(self.range.end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Composition;
    use crate::select::Selection;

    #[test]
    fn a_reported_cursor_is_shifted_into_the_document() {
        let mut composition = Composition::started(10, String::new(), Selection::caret(10));
        composition.range = 10..16;
        assert_eq!(composition.caret_for(Some(3..3)), Selection::caret(13));
        assert_eq!(composition.caret_for(Some(0..6)), Selection::new(10, 16));
    }

    #[test]
    fn a_cursor_past_the_provisional_text_is_clamped_to_it() {
        let mut composition = Composition::started(4, String::new(), Selection::caret(4));
        composition.range = 4..7;
        assert_eq!(composition.caret_for(Some(99..99)), Selection::caret(7));
    }

    #[test]
    fn no_reported_cursor_puts_the_caret_at_the_end_of_what_is_being_composed() {
        let mut composition = Composition::started(0, String::new(), Selection::caret(0));
        composition.range = 0..9;
        assert_eq!(composition.caret_for(None), Selection::caret(9));
    }

    #[test]
    fn the_restore_range_is_what_the_composition_displaced() {
        let composition = Composition::started(2, "abc".to_owned(), Selection::new(2, 5));
        assert_eq!(composition.restore_range(), 2..5);
    }
}
