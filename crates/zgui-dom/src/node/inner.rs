//! The node record: one per node, never moved, read from every worker thread at once.

use core::cell::Cell;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

use selectors::matching::ElementSelectorFlags;
use style::data::ElementDataWrapper;
use stylo_dom::ElementState;
use zgui_bits::{Dirty, DirtyCell};
use zgui_interned::{Ident, NamespaceId};

use crate::arena::store::DocumentStore;
use crate::dirty::children::DirtyChildren;
use crate::id::node_key::{NodeIndex, NodeKey, OptIndex};
use crate::node::atomics;
use crate::node::element::classes::ClassSpan;
use crate::node::element::name::ElementName;
use crate::node::flags::NodeFlags;
use crate::node::kind::NodeKind;
use crate::node_inner;

const _: () = assert!(
    ElementSelectorFlags::all().bits() <= u32::MAX as usize,
    "the engine's selector flags are stored in a 32-bit atomic"
);

node_inner! {
    /// Storage for one node.
    ///
    /// The address is fixed for the record's whole life, because handles into it are copied to
    /// worker threads while the document keeps being built around them.
    ///
    /// Every field is declared through the discipline macro, so its type carries a
    /// [`CellDisciplined`](crate::CellDisciplined) implementation and a borrow counter here would
    /// not compile. The layout is written in two halves, and the split is not cosmetic: the first
    /// holds everything selector matching touches on its hot path, and the invalidation word sits
    /// in it because the ancestor-marking loop reads nothing but that and the parent link.
    pub struct NodeInner {
        // ---- the selector-matching hot set ------------------------------------------------
        /// Back-pointer to the owning store, so a handle stays one word wide.
        doc: *const DocumentStore,
        /// This node's own generation-checked name.
        key: NodeKey,
        /// What this node is.
        kind: NodeKind,
        /// Which namespace this node is in, as an index into the document's namespace table.
        namespace: NamespaceId,
        /// Structural bits, written only between frames.
        flags: Cell<NodeFlags>,
        /// Where this node's pre-split, pre-interned class names live in the document's pool.
        classes: Cell<ClassSpan>,
        /// The tag name, held by value so the engine can borrow it rather than rebuild it.
        local_name: ElementName,
        /// The `id` attribute, as a copyable handle into the document's identifier table.
        id_attr: Cell<Option<Ident>>,
        /// The single source of truth for the state pseudo-classes.
        state: AtomicU64,
        /// This node's own obligations in the low half, its subtree's union in the high half.
        dirty: DirtyCell,

        // ---- links, ordinals and engine bookkeeping ---------------------------------------
        /// Parent link.
        parent: Cell<OptIndex>,
        /// First child of any kind.
        first_child: Cell<OptIndex>,
        /// Last child of any kind.
        last_child: Cell<OptIndex>,
        /// Previous sibling of any kind.
        prev_sibling: Cell<OptIndex>,
        /// Next sibling of any kind.
        next_sibling: Cell<OptIndex>,
        /// Previous element sibling, skipping text and markers.
        prev_element: Cell<OptIndex>,
        /// Next element sibling, skipping text and markers.
        next_element: Cell<OptIndex>,
        /// First element child, skipping text and markers.
        first_element_child: Cell<OptIndex>,
        /// Position among element siblings, numbered lazily and read only through the store.
        ///
        /// Atomic rather than a cell, because the numbering that fills it is taken on a *shared*
        /// borrow the first time anything asks — so two readers can be inside it at once, and a
        /// plain store beside a plain load would be a data race whatever the two of them wrote.
        sibling_ordinal: AtomicU32,
        /// How many children of any kind.
        child_count: Cell<u32>,
        /// Bumped by any structural change under this node.
        ordinals_epoch: Cell<u32>,
        /// The epoch this node's children's ordinals were numbered at.
        ///
        /// Atomic for the same reason as `sibling_ordinal`, and it is the release that publishes
        /// that numbering: it is stored last, so a reader that finds it current is guaranteed to
        /// see every ordinal the numbering wrote.
        ordinals_valid: AtomicU32,
        /// Which element children owe work: up to four exactly, then a span.
        dirty_children: DirtyChildren,
        /// The engine's selector flags, written by any worker, on this node *and on its parent*.
        selector_flags: AtomicU32,
        /// The engine's bookkeeping bits, written by any worker.
        atomics: AtomicU32,
        /// The post-order traversal's outstanding-child counter.
        children_to_process: AtomicI32,
        /// The engine's own per-element style data.
        data: ElementDataWrapper,
    }
}

// SAFETY: what the per-field assertions the declaration emits cannot state, in two parts. First,
// `doc` is only ever dereferenced immutably and the store outlives every record it holds, so
// following it from a worker reads memory nobody writes. Second, a shared reference is the sole
// access path in existence while the style traversal runs, because no exclusive borrow of the
// document may be held across it — which is what makes the cells' between-traversals-only stores
// true rather than hoped for.
unsafe impl Send for NodeInner {}
// SAFETY: as above.
unsafe impl Sync for NodeInner {}

impl NodeInner {
    /// A fresh record for a node of `kind`, owned by `doc`.
    pub(crate) fn new(
        doc: *const DocumentStore,
        key: NodeKey,
        kind: NodeKind,
        local_name: ElementName,
    ) -> Self {
        Self {
            doc,
            key,
            kind,
            namespace: NamespaceId::NONE,
            flags: Cell::new(NodeFlags::empty()),
            classes: Cell::new(ClassSpan::EMPTY),
            local_name,
            id_attr: Cell::new(None),
            state: AtomicU64::new(ElementState::empty().bits()),
            dirty: DirtyCell::clean(),
            parent: Cell::new(OptIndex::NONE),
            first_child: Cell::new(OptIndex::NONE),
            last_child: Cell::new(OptIndex::NONE),
            prev_sibling: Cell::new(OptIndex::NONE),
            next_sibling: Cell::new(OptIndex::NONE),
            prev_element: Cell::new(OptIndex::NONE),
            next_element: Cell::new(OptIndex::NONE),
            first_element_child: Cell::new(OptIndex::NONE),
            sibling_ordinal: AtomicU32::new(0),
            child_count: Cell::new(0),
            ordinals_epoch: Cell::new(0),
            ordinals_valid: AtomicU32::new(0),
            dirty_children: DirtyChildren::empty(),
            selector_flags: AtomicU32::new(0),
            atomics: AtomicU32::new(0),
            children_to_process: AtomicI32::new(0),
            data: ElementDataWrapper::default(),
        }
    }

    /// This node's generation-checked name.
    pub fn key(&self) -> NodeKey {
        self.key
    }

    /// This node's slot number.
    pub fn index(&self) -> NodeIndex {
        NodeIndex::new(self.key.index())
    }

    /// What this node is.
    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    /// Which namespace this node is in.
    pub fn namespace_id(&self) -> NamespaceId {
        self.namespace
    }

    /// This node's tag name.
    pub fn local_name(&self) -> &ElementName {
        &self.local_name
    }

    /// The structural bits.
    pub fn flags(&self) -> NodeFlags {
        self.flags.get()
    }

    /// Sets the structural bits.
    ///
    /// Written only between frames, under an exclusive borrow of the document.
    pub(crate) fn set_flags(&self, flags: NodeFlags) {
        self.flags.set(flags);
    }

    /// Whether every bit of `flags` is set.
    pub fn has_flags(&self, flags: NodeFlags) -> bool {
        self.flags.get().contains(flags)
    }

    /// Where this node's class names live in the document's pool.
    pub fn class_span(&self) -> ClassSpan {
        self.classes.get()
    }

    /// The `id` attribute, if this node has one.
    pub fn id_attr(&self) -> Option<Ident> {
        self.id_attr.get()
    }

    /// The interaction state driving the state pseudo-classes.
    pub fn state(&self) -> ElementState {
        ElementState::from_bits_retain(self.state.load(Ordering::Relaxed))
    }

    /// The same word, in the vocabulary every layer above this one speaks.
    ///
    /// The state is written from above — input routing decides what is hovered and what is
    /// focused — and read from above too: whether an element can take focus at all begins with
    /// whether it is disabled. Both directions have to be expressible without naming the style
    /// engine's own types, which is why the setter takes this form and why the reader answers in
    /// it.
    ///
    /// ```
    /// use zgui_dom::{Document, EverythingMatters, NodeKind};
    /// use zgui_interned::ElementName;
    /// use zgui_vocab::UiState;
    ///
    /// let mut document = Document::new();
    /// let root = document.append(
    ///     document.document_index(),
    ///     NodeKind::Element,
    ///     ElementName::new("root"),
    /// );
    /// document
    ///     .edit(&EverythingMatters, |edit| {
    ///         edit.set_state(root, UiState::DISABLED, true);
    ///     })
    ///     .expect("not poisoned");
    ///
    /// assert!(document.store().core(root).ui_state().contains(UiState::DISABLED));
    /// ```
    pub fn ui_state(&self) -> zgui_vocab::UiState {
        crate::node::element::state::from_engine(self.state())
    }

    /// Replaces the interaction state.
    pub fn set_state(&self, state: ElementState) {
        self.state.store(state.bits(), Ordering::Relaxed);
    }

    /// This node's invalidation word.
    pub fn dirty(&self) -> &DirtyCell {
        &self.dirty
    }

    /// Which of this node's element children owe work.
    pub fn dirty_children(&self) -> &DirtyChildren {
        &self.dirty_children
    }

    /// Whether anything at or below this node owes any of `bits`.
    pub fn has_dirty_descendants(&self, bits: Dirty) -> bool {
        self.dirty.subtree().intersects(bits)
    }

    /// The selector flags the engine has recorded on this node.
    pub fn selector_flags(&self) -> ElementSelectorFlags {
        ElementSelectorFlags::from_bits_retain(self.selector_flags.load(Ordering::Relaxed) as usize)
    }

    /// Adds selector flags, which any worker may do to any node at any time.
    ///
    /// This is the write that forces the field to be an atomic rather than a cell: the engine sets
    /// flags on the element it is matching *and on that element's parent*, from whichever thread
    /// happens to be holding the child, while other threads are reading the parent.
    pub fn insert_selector_flags(&self, flags: ElementSelectorFlags) {
        self.selector_flags
            .fetch_or(flags.bits() as u32, Ordering::Relaxed);
    }

    /// Whether every bit of `bits` is set in the engine's bookkeeping word.
    pub fn has_atomic(&self, bits: u32) -> bool {
        self.atomics.load(Ordering::Relaxed) & bits == bits
    }

    /// Sets bits in the engine's bookkeeping word.
    pub fn set_atomic(&self, bits: u32) {
        self.atomics.fetch_or(bits, Ordering::Relaxed);
    }

    /// Clears bits in the engine's bookkeeping word.
    pub fn clear_atomic(&self, bits: u32) {
        self.atomics.fetch_and(!bits, Ordering::Relaxed);
    }

    /// Whether this node's style data has been established and not cleared.
    pub fn is_styled(&self) -> bool {
        self.has_atomic(atomics::STYLED)
    }

    /// Records how many children the post-order pass still owes this node.
    pub fn store_children_to_process(&self, count: i32) {
        debug_assert_eq!(
            self.children_to_process.load(Ordering::Relaxed),
            0,
            "a counter is only ever stored into once per traversal"
        );
        self.children_to_process.store(count, Ordering::Relaxed);
    }

    /// Records that one child finished, and returns what is left.
    pub fn did_process_child(&self) -> i32 {
        self.children_to_process.fetch_sub(1, Ordering::Relaxed) - 1
    }

    /// The engine's per-element style data.
    pub fn data(&self) -> &ElementDataWrapper {
        &self.data
    }

    /// The store that owns this node.
    ///
    /// # Safety
    ///
    /// The caller chooses the lifetime, and it must not outlive the store. Callers that hold a
    /// handle already have a borrow that proves it, and use it.
    pub unsafe fn store<'doc>(&self) -> &'doc DocumentStore {
        // SAFETY: the pointer was set from the store that owns this record, is never written
        // again, and is only ever read.
        unsafe { &*self.doc }
    }
}

// The record is a controlled quantity: a field added to it is paid for on every node of every
// document, so its size is pinned rather than merely observed, and a field added without noticing
// stops the build. Release only, because the engine's style data carries an extra borrow token in
// debug builds and the record is eight bytes larger there.
//
// Where the bytes go, in release: sixty-four for the hot set that ends at the invalidation word,
// thirty-two for the eight links, sixteen for the two ordinals and two counters, twenty for the
// dirty-child record, twelve for the two engine words and the post-order counter, four of padding,
// and twenty-four for the engine's style data.
#[cfg(not(debug_assertions))]
const _: () = assert!(size_of::<NodeInner>() == 168);

#[cfg(test)]
mod tests {
    use super::NodeInner;

    #[test]
    fn the_hot_half_of_the_record_is_one_cache_line() {
        // The fields up to and including the invalidation word are what the ancestor walk and
        // selector matching touch; everything after is links, ordinals and engine bookkeeping.
        assert_eq!(core::mem::offset_of!(NodeInner, parent), 64);
    }

    #[test]
    fn the_back_pointer_is_first_because_every_method_starts_there() {
        assert_eq!(core::mem::offset_of!(NodeInner, doc), 0);
    }
}
