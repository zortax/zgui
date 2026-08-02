//! The pre-mutation records one restyle consumes.
//!
//! The engine works out what a change can affect by comparing each changed element with a record
//! of how it was. Without such a record it re-matches the changed element and *only* the changed
//! element, so every selector that reached the element sideways — through a sibling combinator,
//! through a descendant combinator — keeps the answer it had. The symptom is that
//! `.item:hover + .label` never lights up, with nothing anywhere reporting a problem.
//!
//! The records are *taken* from the document rather than borrowed, so that the restyle owns them
//! for its whole duration: a change made while a restyle runs belongs to the next restyle and
//! starts a fresh set rather than being added to the one in flight. They are cleared when the
//! restyle finishes, because a record that outlives the change it describes makes the *next*
//! change compare against the wrong past.

use zgui_dom::{Document, DocumentStore, SnapshotStore};

/// The records one restyle owns, cleared when it hands them back.
pub(crate) struct RestyleSnapshots {
    /// The records themselves.
    store: SnapshotStore,
}

impl RestyleSnapshots {
    /// Takes the records waiting on `document`.
    pub(crate) fn take(document: &mut Document) -> Self {
        Self {
            store: document.take_snapshots(),
        }
    }

    /// The records, in the shape the engine's context wants them.
    pub(crate) fn map(&self) -> &style::selector_parser::SnapshotMap {
        self.store.map()
    }

    /// How many elements have a record.
    pub(crate) fn len(&self) -> usize {
        self.store.len()
    }

    /// Discards the records and the bookkeeping bits that say an element has one.
    pub(crate) fn finish(mut self, store: &DocumentStore) {
        self.store.clear(store);
    }
}
