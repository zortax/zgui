//! Undo and redo.
//!
//! The stack holds *changes*, not snapshots: a document of any size costs one range, the bytes
//! that were there and the bytes that are there now. Consecutive typing is folded into one entry
//! so that undo steps by phrase rather than by letter, and every folding rule is stated in
//! [`coalesce`].

pub mod coalesce;
pub mod entry;

pub use crate::history::entry::{EditKind, Entry};

/// The recorded changes, and the ones that have been undone.
///
/// A new change clears the redo stack, which is the behaviour every editor has and the only one
/// that keeps redo meaning "the thing you just undid".
#[derive(Clone, Debug, Default)]
pub struct History {
    /// What has been done, oldest first.
    done: Vec<Entry>,
    /// What has been undone, in the order it would be redone.
    undone: Vec<Entry>,
    /// Whether the next change starts a fresh entry whatever it looks like.
    sealed: bool,
}

impl History {
    /// A history with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many entries can be undone.
    pub fn len(&self) -> usize {
        self.done.len()
    }

    /// Whether there is nothing to undo.
    pub fn is_empty(&self) -> bool {
        self.done.is_empty()
    }

    /// Whether there is anything to redo.
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Ends the current entry, so the next change starts a new one.
    ///
    /// Called wherever something happened that a person would not expect an undo to step over: the
    /// caret was moved with the pointer, the field lost focus, a composition started. Without it,
    /// coalescing joins two changes that only look adjacent.
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    /// Records a change, folding it into the previous entry when the rules in [`coalesce`] allow.
    ///
    /// Returns whether a new entry was started, which is what tells a caller whether the state
    /// before this change is one an undo will come back to.
    pub fn record(&mut self, change: Entry) -> bool {
        self.undone.clear();
        let joined = !self.sealed
            && self
                .done
                .last()
                .is_some_and(|last| coalesce::joins(last, &change));
        self.sealed = false;
        if joined {
            let last = self.done.last_mut().expect("just checked");
            coalesce::fold(last, change);
            return false;
        }
        self.done.push(change);
        true
    }

    /// Takes the next change to undo, moving it onto the redo stack.
    pub fn undo(&mut self) -> Option<Entry> {
        let entry = self.done.pop()?;
        self.undone.push(entry.clone());
        self.sealed = true;
        Some(entry)
    }

    /// Takes the next change to redo, moving it back onto the undo stack.
    pub fn redo(&mut self) -> Option<Entry> {
        let entry = self.undone.pop()?;
        self.done.push(entry.clone());
        self.sealed = true;
        Some(entry)
    }

    /// Forgets everything, which is what loading a new document into a field does.
    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
        self.sealed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{EditKind, Entry, History};
    use crate::select::Selection;

    /// An insertion of `inserted` at `at`.
    fn typed(at: usize, inserted: &str) -> Entry {
        Entry {
            range: at..at,
            removed: String::new(),
            inserted: inserted.to_owned(),
            before: Selection::caret(at),
            after: Selection::caret(at + inserted.len()),
            kind: EditKind::Insert,
        }
    }

    #[test]
    fn typing_a_word_is_one_undo() {
        let mut history = History::new();
        assert!(history.record(typed(0, "a")));
        assert!(!history.record(typed(1, "b")));
        assert!(!history.record(typed(2, "c")));
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn sealing_forces_the_next_change_to_start_its_own_entry() {
        let mut history = History::new();
        history.record(typed(0, "a"));
        history.seal();
        assert!(history.record(typed(1, "b")));
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn undoing_and_typing_again_throws_away_the_redo() {
        let mut history = History::new();
        history.record(typed(0, "a"));
        assert!(history.undo().is_some());
        assert!(history.can_redo());
        history.record(typed(0, "z"));
        assert!(!history.can_redo());
    }

    #[test]
    fn an_undone_change_can_be_redone_and_undone_again() {
        let mut history = History::new();
        history.record(typed(0, "a"));
        let undone = history.undo().expect("one entry");
        assert!(history.is_empty());
        let redone = history.redo().expect("one undone entry");
        assert_eq!(undone, redone);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn an_undo_leaves_a_boundary_so_the_next_change_does_not_join_what_survived() {
        let mut history = History::new();
        history.record(typed(0, "a"));
        history.record(typed(1, "b"));
        history.undo();
        // The stack is empty, but the seal matters again as soon as anything is left on it.
        history.record(typed(0, "x"));
        assert_eq!(history.len(), 1);
    }
}
