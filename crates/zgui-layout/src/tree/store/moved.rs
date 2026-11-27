//! The fragments layout moved.

use zgui_space::ChangeCoverage;

use crate::fragment::FragKey;
use crate::tree::store::{JournalVersion, LayoutStore};

impl LayoutStore {
    /// Returns the current version of the move journal.
    pub fn moves_version(&self) -> JournalVersion {
        self.moved.version()
    }

    /// Appends every fragment whose border box moved or resized since `version` into `moved`.
    ///
    /// A fragment layout added may be appended too, a fragment moved more than once may be appended
    /// more than once, and a fragment appended may have been removed since.
    ///
    /// [`ChangeCoverage::All`] means the caller must treat every fragment as moved instead. It is
    /// returned for a version of another store, and for a version older than the history the
    /// store keeps, which is at least as many moves as there are fragments.
    pub fn moved_since(&self, version: JournalVersion, moved: &mut Vec<FragKey>) -> ChangeCoverage {
        self.moved.since(version, moved)
    }

    /// Records that the border box of `key` moved or resized.
    pub(crate) fn note_moved(&mut self, key: FragKey) {
        let history = self.fragments.len() as usize;
        self.moved.mark_keeping(key, history);
    }
}
