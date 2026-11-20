//! The paint order mirrored as the compositor's order tree.

use compositor_types::{ChunkKey, OrderEntry, OrderId, OrderNode, OutputId, Transaction};
use zgui_dom::side::BoxKey;
use zgui_layout::LayoutStore;

use crate::sync::key::{self, Part};
use crate::walk::stacking::{self, Visitor};

/// The order node an output draws, holding every fragment's chunk in painting order.
#[derive(Debug, Default)]
pub(crate) struct Order {
    root: Option<OrderId>,
    entries: Vec<ChunkKey>,
    scratch: Vec<ChunkKey>,
    dirty: bool,
}

impl Order {
    /// Creates the root node and makes `output` draw it, unless that is done.
    pub(crate) fn ensure_root(&mut self, tx: &mut Transaction, output: OutputId) {
        if self.root.is_none() {
            let root = tx.create_order(OrderNode::default());
            tx.set_root(output, Some(root));
            self.root = Some(root);
        }
    }

    /// Notes that the set of chunks or their order may have changed.
    pub(crate) fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Rewrites the root's sequence to the painting order of `store`, if it was invalidated.
    ///
    /// The old and new sequences are compared, and the part between their common prefix and
    /// suffix is replaced with one splice. Returns the number of entries replaced.
    pub(crate) fn sync(&mut self, tx: &mut Transaction, store: &LayoutStore) -> usize {
        let Some(root) = self.root else {
            return 0;
        };
        if !std::mem::take(&mut self.dirty) {
            return 0;
        }
        self.scratch.clear();
        if let Some(top) = store.root() {
            stacking::walk(
                store,
                top,
                &mut Collect {
                    out: &mut self.scratch,
                },
            );
        }
        let (old, new) = (&self.entries, &self.scratch);
        let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let removed = old.len() - prefix - suffix;
        let inserted: Vec<OrderEntry> = new[prefix..new.len() - suffix]
            .iter()
            .map(|key| OrderEntry::Chunk(*key))
            .collect();
        if removed > 0 || !inserted.is_empty() {
            tx.splice(root, prefix as u32, removed as u32, &inserted);
        }
        std::mem::swap(&mut self.entries, &mut self.scratch);
        removed + inserted.len()
    }
}

/// Collects every fragment's chunk key in painting order.
struct Collect<'a> {
    out: &'a mut Vec<ChunkKey>,
}

impl Visitor for Collect<'_> {
    fn enter(&mut self, store: &LayoutStore, key: BoxKey) -> bool {
        let count = store.fragments_of_box(key).len() as u32;
        self.out
            .extend((0..count).map(|ordinal| key::at(key.as_u64(), ordinal, Part::Body)));
        true
    }

    fn leave(&mut self, _store: &LayoutStore, _key: BoxKey) {}
}
