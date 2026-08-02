//! Where boxes, their results and their fragments live.
//!
//! | Module | Contents |
//! |---|---|
//! | `resolved` | one box's geometry, restated in this framework's own units |
//! | `state` | one box's layout-time state: the engine's cache, and what it produced |
//! | `fragments` | the pieces boxes were painted as |
//! | `inline` | what an inline formatting context resolved to |
//! | `scroll` | which axes reserve a scrollbar gutter |
//! | `laid_out` | the viewport the results now held were produced for |
//! | `measured` | the size-only answers a box keeps beyond the engine's own nine slots |

mod fragments;
mod inline;
pub mod laid_out;
pub(crate) mod measured;
mod resolved;
mod scroll;
pub(crate) mod state;

#[cfg(test)]
mod tests;

use zgui_arena::{ArenaKind, ChunkArena, DocumentId, DomainId, PagedVec, SlotVec};
use zgui_dom::NodeKey;
use zgui_dom::side::{BoxKey, BoxList};

use crate::fragment::{FragKey, FragList, Fragment};
use crate::key::{named, slot};
use crate::node::box_node::BoxNode;
use crate::tree::store::state::BoxLayout;

pub use crate::tree::store::resolved::ResolvedLayout;

/// Which of a document's arenas holds its boxes.
///
/// Every arena of a document indexes independently, so the same slot number occurs in all of them
/// at once; naming each arena once is what keeps a handle from resolving inside a key space it does
/// not belong to.
pub const BOX_ARENA: ArenaKind = match ArenaKind::new(1) {
    Some(kind) => kind,
    None => panic!("one is a valid arena kind"),
};

/// Which of a document's arenas holds its fragments.
pub const FRAGMENT_ARENA: ArenaKind = match ArenaKind::new(2) {
    Some(kind) => kind,
    None => panic!("two is a valid arena kind"),
};

/// Every box of one document, every box's result, and every fragment those results produced.
///
/// The store outlives a layout pass. What a pass borrows is the store plus whatever it needs to
/// measure content with; the boxes, the caches and the fragments stay put between frames, which is
/// what makes an incremental pass cheaper than a fresh one.
#[derive(Debug)]
pub struct LayoutStore {
    /// The box records.
    boxes: ChunkArena<BoxNode>,
    /// One entry per box, holding the engine's cache and the box's result.
    layout: SlotVec<BoxKey, BoxLayout>,
    /// The box the document is laid out from.
    root: Option<BoxKey>,
    /// Which boxes each element generated, in document order.
    boxes_of_node: PagedVec<NodeKey, BoxList>,
    /// The fragment records.
    fragments: ChunkArena<Fragment>,
    /// Which fragments each element's boxes produced.
    fragments_of_node: PagedVec<NodeKey, FragList>,
    /// The shaped paragraphs the document's inline formatting contexts resolved to.
    ///
    /// A fragment names a paragraph by its position here rather than by the key its glyphs are
    /// cached under, so that the identifier a fragment carries stays small and stays this store's
    /// to issue — there is one paragraph identity in the document, and it is this one.
    paragraphs: Vec<zgui_text::ParagraphKey>,
    /// The fragments destroyed since the last pass drained this list.
    ///
    /// A fragment ceases to exist for two quite different reasons — a box produced fewer pieces
    /// than last time, or the box itself was taken out of the tree — and only the first is
    /// something the walk that composes fragments ever sees. Recording both here, at the one place
    /// a fragment is actually destroyed, is what lets that walk take the names out of the hit index
    /// whether or not it ever visited the box they belonged to.
    retired: Vec<FragKey>,
    /// The boxes taken out of the tree since the last pass drained this list.
    ///
    /// Recorded for the same reason the fragments are, and read by whoever holds something *named
    /// after* a box: a coordinate system is named after the box that establishes it, and a walk that
    /// composes only what changed never visits a box that is no longer there to be told it has gone.
    /// Without this the names of deleted boxes are never given back, and the dense buffer they are
    /// the index of grows for the life of the process.
    retired_boxes: Vec<BoxKey>,
    /// Every fragment whose filter chain reads pixels outside every rectangle it writes.
    ///
    /// Sparse: empty in almost every document, a handful of entries in one with a blurred dialog
    /// or a frosted header. It is maintained as fragments gain, lose or are destroyed with such an
    /// extent rather than rebuilt, because a fragment that did not change this frame is never
    /// revisited and rebuilding would lose it exactly when the content beneath it animates.
    read_extents: Vec<FragKey>,
    /// How many inline formatting contexts have been flattened into a shaper's string.
    flattenings: u64,
    /// The viewport the last root layout that ran to completion was asked for, by its bits.
    ///
    /// See [`laid_out`](crate::tree::store::laid_out) for what it is compared against and why it is
    /// held here rather than beside the pass.
    laid_out: Option<(u32, u32)>,
}

impl LayoutStore {
    /// An empty store for one document.
    pub fn new(document: DocumentId) -> Self {
        let box_domain = DomainId::new(document, BOX_ARENA);
        let fragment_domain = DomainId::new(document, FRAGMENT_ARENA);
        Self {
            boxes: ChunkArena::new(box_domain),
            layout: SlotVec::for_domain(box_domain),
            root: None,
            boxes_of_node: PagedVec::for_domain(zgui_dom::id::document_id::node_domain(document)),
            fragments: ChunkArena::new(fragment_domain),
            fragments_of_node: PagedVec::for_domain(zgui_dom::id::document_id::node_domain(
                document,
            )),
            paragraphs: Vec::new(),
            retired: Vec::new(),
            retired_boxes: Vec::new(),
            read_extents: Vec::new(),
            flattenings: 0,
            laid_out: None,
        }
    }

    /// The arena boxes are named in.
    pub fn box_domain(&self) -> DomainId {
        self.boxes.domain()
    }

    /// How many boxes are live.
    pub fn len(&self) -> u32 {
        self.boxes.len()
    }

    /// Whether the document generated no boxes.
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// The box the document is laid out from, if the tree has been built.
    pub fn root(&self) -> Option<BoxKey> {
        self.root
    }

    /// Makes `key` the box the document is laid out from.
    pub fn set_root(&mut self, key: BoxKey) {
        self.root = Some(key);
    }

    /// Adds a box, and gives it an empty result to be laid out into.
    pub fn insert(&mut self, node: BoxNode) -> BoxKey {
        let source = node.source;
        let key = named(self.boxes.insert(node));
        self.layout.insert(key, BoxLayout::default());
        if let Some(source) = source {
            self.boxes_of_node.get_mut(source).push(key);
        }
        key
    }

    /// The record for one box.
    pub fn get(&self, key: BoxKey) -> Option<&BoxNode> {
        self.boxes.get(slot(key))
    }

    /// The record for one box, for modification.
    pub fn get_mut(&mut self, key: BoxKey) -> Option<&mut BoxNode> {
        self.boxes.get_mut(slot(key))
    }

    /// The record for one box, which must exist.
    ///
    /// # Panics
    ///
    /// If the key names no live box.
    pub fn node(&self, key: BoxKey) -> &BoxNode {
        self.get(key).expect("a live box key")
    }

    /// Whether a key names a live box.
    pub fn contains(&self, key: BoxKey) -> bool {
        self.boxes.contains_key(slot(key))
    }

    /// Removes one box. Its record stays readable until the frame is recycled.
    pub fn remove(&mut self, key: BoxKey) -> bool {
        self.retired_boxes.push(key);
        // The pieces this box was painted as cease to exist with it, and everything that named
        // them — the read-extent registry, the element's own list — has to be told.
        self.clear_fragments(key);
        if let Some(source) = self.get(key).and_then(|node| node.source) {
            self.boxes_of_node
                .get_mut(source)
                .retain(|&mut it| it != key);
        }
        self.layout.remove(key);
        if self.root == Some(key) {
            self.root = None;
        }
        self.boxes.remove(slot(key))
    }

    /// Takes the boxes removed since the last call, leaving the list empty.
    ///
    /// Drained rather than read so that each removed box is handed out exactly once: a second
    /// reader would give back a name a third party has since been issued.
    pub fn drain_retired_boxes(&mut self) -> Vec<BoxKey> {
        core::mem::take(&mut self.retired_boxes)
    }

    /// Drops this frame's removed boxes and fragments and returns their slots to the allocator.
    pub fn recycle(&mut self) {
        self.boxes.recycle();
        self.fragments.recycle();
    }

    /// The boxes one element generated, in document order.
    pub fn boxes_of(&self, node: NodeKey) -> &[BoxKey] {
        self.boxes_of_node.get(node).map_or(&[], |list| &list[..])
    }

    /// Walks every live box, in no particular order.
    pub fn keys(&self) -> Vec<BoxKey> {
        let mut keys = Vec::new();
        if let Some(root) = self.root {
            let mut stack = vec![root];
            while let Some(key) = stack.pop() {
                keys.push(key);
                stack.extend(self.node(key).children.iter().copied());
            }
        }
        keys
    }
}
