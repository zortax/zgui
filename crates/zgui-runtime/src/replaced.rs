//! One [`ReplacedContent`] source for the document, multiplexing every owner of outside content.
//!
//! The document takes exactly one intrinsic-sizing source, and more than one thing in a window
//! owns content the document cannot see: decoded images are one owner, embedded surfaces another.
//! Each owner writes the intrinsics of *its* nodes into its own [`IntrinsicTable`]; the mux is the
//! one object installed on the document, and it answers with the first table that knows the node.
//!
//! Presence is the tie-break, not the value: a node belongs to whichever owner filed it, even
//! while that owner's honest answer is still [`Intrinsic::default()`] — an image before its
//! decode lands is *known and unsized*, which is a different thing from unknown.
//!
//! Everything here is `Send + Sync` because the trait demands it: intrinsics are read from layout
//! workers. Writes happen between frames, so the locks are never contended for long.

use std::sync::{Arc, RwLock};

use rustc_hash::FxHashMap;
use zgui_dom::host::replaced::{Intrinsic, ReplacedContent, ReplacedId};

/// The intrinsics of one owner's replaced nodes, shared between the owner and the mux.
///
/// The owner keeps a clone and writes through it; the mux holds the other and reads. Entries are
/// the owner's to remove: a table is a statement of which nodes are the owner's, so leaving a dead
/// node's entry behind claims a node that may since have become someone else's.
#[derive(Default)]
pub struct IntrinsicTable {
    entries: RwLock<FxHashMap<ReplacedId, Intrinsic>>,
}

impl IntrinsicTable {
    /// An empty table.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Files `id` as this owner's, with what is known of its content so far.
    ///
    /// Filing with `Intrinsic::default()` is meaningful: it claims the node while saying its
    /// natural size is not yet known.
    pub fn set(&self, id: ReplacedId, intrinsic: Intrinsic) {
        self.entries
            .write()
            .expect("an intrinsic write never panics mid-write")
            .insert(id, intrinsic);
    }

    /// Removes `id` from this owner's nodes.
    pub fn remove(&self, id: ReplacedId) {
        self.entries
            .write()
            .expect("an intrinsic write never panics mid-write")
            .remove(&id);
    }

    /// What this table knows about `id`, if the node is this owner's at all.
    pub fn get(&self, id: ReplacedId) -> Option<Intrinsic> {
        self.entries
            .read()
            .expect("an intrinsic write never panics mid-write")
            .get(&id)
            .copied()
    }
}

/// The document's one replaced-content source, answering from the owners' tables in order.
pub struct ReplacedMux {
    /// The owners' tables. Order is precedence, though a node filed in two tables is a bug in
    /// whichever owner filed it second.
    tables: Vec<Arc<IntrinsicTable>>,
}

impl ReplacedMux {
    /// A mux over `tables`.
    pub fn new(tables: Vec<Arc<IntrinsicTable>>) -> Self {
        Self { tables }
    }
}

impl ReplacedContent for ReplacedMux {
    fn intrinsic(&self, id: ReplacedId) -> Intrinsic {
        self.tables
            .iter()
            .find_map(|table| table.get(id))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use zgui_arena::{DomainId, Generation};
    use zgui_dom::NodeKey;

    use super::*;

    fn id(n: u32) -> ReplacedId {
        ReplacedId::new(NodeKey::new(n, Generation::FIRST, DomainId::FIRST))
    }

    #[test]
    fn the_first_table_that_knows_the_node_answers() {
        let images = IntrinsicTable::new();
        let surfaces = IntrinsicTable::new();
        let mux = ReplacedMux::new(vec![Arc::clone(&images), Arc::clone(&surfaces)]);

        let filed = Intrinsic {
            ratio: Some(2.0),
            ..Intrinsic::default()
        };
        surfaces.set(id(1), filed);
        assert_eq!(mux.intrinsic(id(1)).ratio, Some(2.0));
        assert_eq!(mux.intrinsic(id(2)), Intrinsic::default());
    }

    #[test]
    fn filing_nothing_is_still_a_claim_and_removal_withdraws_it() {
        let images = IntrinsicTable::new();
        images.set(id(3), Intrinsic::default());
        assert_eq!(images.get(id(3)), Some(Intrinsic::default()));
        images.remove(id(3));
        assert_eq!(images.get(id(3)), None);
    }
}
