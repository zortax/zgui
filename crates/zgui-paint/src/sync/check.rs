//! The check, with invariants switched on, that the order tree lists every recorded chunk.

use zgui_layout::LayoutStore;

use crate::sync::{CompositorSync, left};

impl CompositorSync {
    /// Panics when a live box with a recorded chunk is held by no list of the order tree.
    pub(super) fn check_listed(&self, store: &LayoutStore) {
        for (&chunk, &fragment) in &self.owners {
            let Some(fragment) = store.fragment(fragment) else {
                continue;
            };
            let key = fragment.box_;
            if store.removed(key) || left(store, key) {
                continue;
            }
            assert!(
                self.order.knows(key),
                "the chunk {chunk:?} of the box {key:?} is in no list of the order tree"
            );
        }
    }
}
