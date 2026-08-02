//! One undoable step.

use core::ops::Range;

use crate::select::Selection;

/// What kind of change an entry records, which is what decides whether the next one joins it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditKind {
    /// Text was typed in.
    Insert,
    /// Text was removed backwards, which is what the backspace key does.
    DeleteBackwards,
    /// Text was removed forwards, which is what the delete key does.
    DeleteForwards,
    /// Anything else: a paste, a drop, a replacement of a selection, a committed composition.
    Replace,
}

impl EditKind {
    /// Whether two consecutive edits of this kind may be recorded as one.
    ///
    /// Typing and backspacing coalesce, so a sentence typed in is one undo and not forty. A
    /// replacement never does: it is a deliberate act, and joining a paste to the letter typed
    /// before it makes one undo throw away both.
    pub const fn coalesces(self) -> bool {
        matches!(
            self,
            Self::Insert | Self::DeleteBackwards | Self::DeleteForwards
        )
    }
}

/// One recorded change: what was replaced, by what, and where the caret was on each side.
///
/// The selection is part of the record rather than derived from the range, because undo restores
/// the caret to where it was and no rule over the range alone gets that right: the caret that
/// produced a backspace was after the removed text, and the one that produced a forward delete was
/// before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The range of the text this replaced, before it was applied.
    pub range: Range<usize>,
    /// What was there.
    pub removed: String,
    /// What is there now.
    pub inserted: String,
    /// The selection before the change.
    pub before: Selection,
    /// The selection after it.
    pub after: Selection,
    /// What kind of change it was.
    pub kind: EditKind,
}

impl Entry {
    /// The range the inserted text occupies now.
    pub fn inserted_range(&self) -> Range<usize> {
        self.range.start..self.range.start + self.inserted.len()
    }
}
