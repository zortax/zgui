//! The boxes the order tree holds: those drawn since they arrived.
//!
//! A box is drawn once a chunk is recorded in its subtree or it is drawn with a group effect. The
//! boxes the walk never reaches, such as the rows of a long list outside the surface, stay out of
//! the tree until a chunk of theirs is recorded.

use zgui_dom::side::BoxKey;
use zgui_layout::LayoutStore;

use super::Order;

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
            let mut at = key;
            while !self.knows(at) && self.scratch.shown.insert(at) {
                match store.get(at).and_then(|node| node.parent) {
                    Some(parent) => at = parent,
                    None => break,
                }
            }
            if at != key && self.knows(at) {
                work.push(at);
            }
        }
        drawn.clear();
        self.drawn = drawn;
    }
}
