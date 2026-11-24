//! The boxes the order tree holds: those drawn since they arrived.
//!
//! A box is drawn once a chunk is recorded in its subtree or it is drawn with a group effect. The
//! boxes the walk never reaches, such as the rows of a long list outside the surface, stay out of
//! the tree until a chunk of theirs is recorded.

use zgui_dom::side::BoxKey;
use zgui_layout::LayoutStore;

use super::{Context, Order};

impl Order {
    /// Notes that the box `key` is drawn.
    pub(crate) fn recorded(&mut self, key: BoxKey) {
        if !self.knows(key) {
            self.drawn.push(key);
        }
    }

    /// Reports whether the current sync lists the box `key`.
    pub(super) fn shows(&self, key: BoxKey) -> bool {
        self.knows(key) || self.scratch.shown.contains(&key)
    }

    /// Shows the boxes drawn since the last sync and the boxes above them that no list holds, and
    /// appends to `work` the first box above each such chain that a list holds.
    ///
    /// The list of that box then gains the top of the chain, whose node lists the rest as it is
    /// created.
    pub(super) fn show_drawn(&mut self, store: &LayoutStore, work: &mut Vec<BoxKey>) {
        let mut drawn = std::mem::take(&mut self.drawn);
        for &key in &drawn {
            if let Some(above) = self.show(store, key) {
                work.push(above);
            }
        }
        drawn.clear();
        self.drawn = drawn;
    }

    /// Lists again each live box that a list dropped in this sync and no list holds now, such as a
    /// box moved under another parent, and returns the entries spliced.
    ///
    /// Every box a list held was drawn, so each is shown again and the list above it rebuilt.
    pub(super) fn relist_dropped(
        &mut self,
        context: &mut Context<'_>,
        work: &mut Vec<BoxKey>,
    ) -> usize {
        let mut entries = 0;
        let mut checked = 0;
        // Rebuilding a list can drop further boxes, which are checked in turn.
        while let Some(&key) = self.scratch.orphans.get(checked) {
            checked += 1;
            let store = context.store;
            if self.listed(key) || store.get(key).is_none() || store.removed(key) {
                continue;
            }
            if let Some(above) = self.show(store, key) {
                self.scratch.visited.remove(&above);
                work.push(above);
                entries += self.drain(context, work);
            }
        }
        entries
    }

    /// Reports whether a list or the output's root holds the box `key`.
    ///
    /// A box keeps its node after a list drops it until the sync removes the nodes no list names.
    fn listed(&self, key: BoxKey) -> bool {
        self.top == Some(key) || self.placed.contains_key(&key)
    }

    /// Shows the box `key` and the boxes above it that no list holds, and returns the first box
    /// above them that a list holds.
    ///
    /// Returns `None` when a list holds `key`, when the chain was shown already, and when it ends
    /// at a box without a parent.
    fn show(&mut self, store: &LayoutStore, key: BoxKey) -> Option<BoxKey> {
        let mut at = key;
        while !self.listed(at) {
            if !self.scratch.shown.insert(at) {
                return None;
            }
            at = store.get(at)?.parent?;
        }
        (at != key).then_some(at)
    }
}
