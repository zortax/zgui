//! Chunks whose fragments layout moved while the walk did not reach them.
//!
//! Such a chunk would stay drawn at the old place, which none of the boxes the walk tests covers
//! any more. It is withdrawn instead, and the fragment is recorded again once the walk reaches it.

use zgui_layout::{FragKey, Fragment, JournalVersion, LayoutStore};
use zgui_space::ChangeCoverage;

use crate::sync::CompositorSync;

/// The fragments layout moved since a walk last read them.
#[derive(Debug, Default)]
pub(super) struct Moves {
    /// The version of the layout's move journal the last walk read.
    read: Option<JournalVersion>,
    /// The fragments moved between the last two reads.
    fragments: Vec<FragKey>,
    /// Whether any fragment can have moved, beyond those listed.
    all: bool,
}

impl CompositorSync {
    /// Reads the fragments `store` moved since the last walk, and reports whether any can have
    /// moved.
    pub(crate) fn read_moves(&mut self, store: &LayoutStore) -> bool {
        let moves = &mut self.moves;
        moves.fragments.clear();
        moves.all = match moves.read {
            Some(version) => {
                store.moved_since(version, &mut moves.fragments) == ChangeCoverage::All
            }
            None => true,
        };
        moves.read = Some(store.moves_version());
        moves.all || !moves.fragments.is_empty()
    }

    /// Withdraws the chunk of each fragment read by the last [`CompositorSync::read_moves`] for
    /// which `moved_away` holds.
    pub(crate) fn withdraw_moved(
        &mut self,
        store: &LayoutStore,
        moved_away: impl Fn(&Fragment) -> bool,
    ) {
        let mut fragments = std::mem::take(&mut self.moves.fragments);
        if self.moves.all {
            fragments.clear();
            fragments.extend(self.chunks.keys().copied());
        }
        for &key in &fragments {
            if self.knows(key)
                && let Some(fragment) = store.fragment(key)
                && moved_away(fragment)
            {
                self.withdraw(key);
            }
        }
        fragments.clear();
        self.moves.fragments = fragments;
    }
}
