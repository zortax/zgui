//! Everything a worker thread can reach from a node handle.

use rustc_hash::FxHashMap;
use style::shared_lock::SharedRwLock;
use style::values::AtomIdent;
use zgui_arena::{ChunkArena, DomainId};
use zgui_interned::{Ident, NamespaceId};

use crate::arena::class_pool::ClassPool;
use crate::arena::columns::Columns;
use crate::host::HostSeams;
use crate::id::document_id::{DocumentId, node_domain};
use crate::id::node_key::{NodeIndex, NodeKey};
use crate::mutate::filter::StyleFilter;
use crate::node::element::classes::ClassSpan;
use crate::node::element::ident::IdentTable;
use crate::node::inner::NodeInner;

/// Everything a worker thread can reach from a node handle.
///
/// A record holds a raw pointer back to the store that owns it, which is what keeps a handle one
/// machine word wide — so every method the style engine calls on an element can reach *all* of
/// this, from several threads at once. The obligation that follows is stated as a compile-time
/// assertion rather than as prose:
///
/// ```
/// zgui_dom::assert_sync::<zgui_dom::DocumentStore>();
/// ```
///
/// What that covers is the store's own fields and its columns, and that is where it bites: a
/// scratch buffer behind a borrow counter parked here, or a reference-counted callback in a column,
/// stops the build. It stops at the record, which carries its own promise for the reasons written
/// there. Per-frame scratch that no engine method can reach belongs on the document rather than
/// here, and is outside the assertion by construction rather than by luck.
pub struct DocumentStore {
    /// The records. Addresses are stable, which is what lets a handle be a reference.
    arena: ChunkArena<NodeInner>,
    /// Slot number to the generation-checked key that names it.
    keys: Vec<NodeKey>,
    /// Every class name of every node, split and interned once.
    classes: ClassPool,
    /// Identifiers, so that a copyable handle in a record resolves to a borrowed atom.
    idents: IdentTable,
    /// The namespaces this document uses, indexed by the one-byte identifier a record holds.
    namespaces: Vec<web_atoms::Namespace>,
    /// Everything about a node that is not copyable.
    columns: Columns,
    /// The lock every stylesheet and every declaration block in this document shares.
    lock: SharedRwLock,
    /// Which document this is.
    document: DocumentId,
    /// Class names already interned, so that rewriting the same classes reuses their run.
    interned_runs: FxHashMap<Box<[AtomIdent]>, ClassSpan>,
    /// The hooks a consumer installs to put a document language on top of this document.
    ///
    /// They live here rather than on the document because every one of them is consulted from a
    /// node handle, and a node handle reaches this and nothing else. There is deliberately no
    /// stylesheet-dependency filter beside them: that one is answered from a compiled rule set,
    /// which cannot be sent between threads, so it is passed to each call instead of stored.
    host: HostSeams,
}

const _: () = crate::assert_sync::<DocumentStore>();

impl DocumentStore {
    /// An empty store for `document`.
    pub fn new(document: DocumentId) -> Self {
        let domain = node_domain(document);
        Self {
            arena: ChunkArena::new(domain),
            keys: Vec::new(),
            classes: ClassPool::new(),
            idents: IdentTable::new(),
            namespaces: vec![web_atoms::Namespace::from("")],
            columns: Columns::new(domain),
            lock: SharedRwLock::new(),
            document,
            interned_runs: FxHashMap::default(),
            host: HostSeams::new(),
        }
    }

    /// The hooks a consumer has installed on this document.
    pub fn host(&self) -> &HostSeams {
        &self.host
    }

    /// The hooks, for installing one.
    pub(crate) fn host_mut(&mut self) -> &mut HostSeams {
        &mut self.host
    }

    /// Drops every cached state mask in the document.
    ///
    /// Called when the stylesheet set changes, which is the one invalidation that is not about a
    /// single element: every cached answer was narrowed against rules that are no longer the rules.
    pub fn invalidate_all_state_masks(&mut self) {
        self.columns.state_mask.compact_by(|_| true);
    }

    /// The interaction-state bits any active selector could match `index` on.
    ///
    /// Writing a bit outside this mask changes nothing that matches, so the write needs no snapshot
    /// and no restyle. The answer is narrowed by the element's own identifier, classes, local name
    /// and root-ness, so it is cached per element — and dropped by
    /// [`DocumentStore::invalidate_state_mask`] inside every write that changes one of those.
    ///
    /// A filter that reports itself unusable — the one frame in which the stylesheet set changed,
    /// during which its index still describes the previous set — is neither asked nor cached, and
    /// every bit is reported as able to matter. Narrowing from an index that describes other rules
    /// is the false-negative direction: a write skipped for a bit that turns out to matter is a
    /// restyle that never happens, and a cached answer from that window would outlive it.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn states_for(
        &mut self,
        index: NodeIndex,
        filter: &dyn StyleFilter,
    ) -> stylo_dom::ElementState {
        if filter.is_disabled() {
            return stylo_dom::ElementState::all();
        }
        let key = self.key_of(index);
        if let Some(Some(cached)) = self.columns.state_mask.get(key) {
            return *cached;
        }
        let mask = filter.states_for(crate::node::handle::Node::new(self.core(index)));
        *self.columns.state_mask.get_mut(key) = Some(mask);
        mask
    }

    /// Forgets `index`'s cached state mask.
    ///
    /// Called from inside the write that changes the element's identity, never afterwards: the
    /// stale read happens in the same batch as the write, so a sweep at the end of the batch is
    /// already too late.
    ///
    /// The writes that owe this call are the ones that change a bucket the answer was narrowed by:
    /// a class change, an identifier change, and **a move that changes whether the element is the
    /// root**. All three call it: [`DocumentStore::write_classes`] and
    /// [`DocumentStore::write_id`] for the first two, and the insertion and removal in a batch of
    /// changes for the third, each inside the write itself.
    ///
    /// A change to the stylesheet set invalidates every answer at once instead, through
    /// [`DocumentStore::invalidate_all_state_masks`].
    ///
    /// # Panics
    ///
    /// Panics if `index` names no slot of this document.
    pub fn invalidate_state_mask(&mut self, index: NodeIndex) {
        let key = self.key_of(index);
        self.columns.state_mask.clear(key);
    }

    /// Which document this is.
    pub fn document(&self) -> DocumentId {
        self.document
    }

    /// The arena this document's node keys belong to.
    pub fn domain(&self) -> DomainId {
        self.arena.domain()
    }

    /// The lock this document's stylesheets and declaration blocks share.
    ///
    /// One lock per document, and every stylesheet, every `style` attribute and every restyle guard
    /// comes from it. A declaration block behind a different lock cannot be read under the guard the
    /// style engine takes, so this is a structural requirement rather than a convenience.
    pub fn lock(&self) -> &SharedRwLock {
        &self.lock
    }

    /// How many nodes the store holds.
    ///
    /// A node removed during the frame in flight still counts: its record is still there and its
    /// key still resolves, and it stops counting when the frame ends and the record is dropped.
    pub fn len(&self) -> usize {
        self.arena.len() as usize
    }

    /// Whether the store holds no nodes at all.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many slot numbers exist, whether or not anything lives in them.
    ///
    /// This is the bound to iterate the slot space with, and it is a high-water mark: it never
    /// decreases, because a slot the recycling pass returned keeps its number for its next
    /// occupant. Use [`DocumentStore::len`] for how many nodes there are.
    pub fn slot_count(&self) -> usize {
        self.keys.len()
    }

    /// The record for `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn core(&self, index: NodeIndex) -> &NodeInner {
        self.try_core(index)
            .expect("a slot number always names a live node of its own document")
    }

    /// The record for `index`, or [`None`] if it names nothing live.
    pub fn try_core(&self, index: NodeIndex) -> Option<&NodeInner> {
        let key = *self.keys.get(index.get() as usize)?;
        self.arena.get(key)
    }

    /// The record `key` names, or [`None`] if the node it named is gone.
    pub fn get(&self, key: NodeKey) -> Option<&NodeInner> {
        self.arena.get(key)
    }

    /// `index`'s generation-checked name.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no slot of this document.
    pub fn key_of(&self, index: NodeIndex) -> NodeKey {
        self.keys[index.get() as usize]
    }

    /// `key`'s slot number, or [`None`] if the node it named is gone.
    pub fn index_of(&self, key: NodeKey) -> Option<NodeIndex> {
        self.arena.get(key).map(NodeInner::index)
    }

    /// `node`'s position among its element siblings, counting from zero.
    ///
    /// Positions are numbered lazily: a structural change marks its parent's child list as owing a
    /// renumber, and this pays for one pass the first time anything asks afterwards. That is the
    /// difference between appending a thousand rows costing one renumber and costing a thousand.
    ///
    /// Read positions through here and never off the record. The window in which the stored number
    /// is stale is not exotic — it opens at every insertion and removal — and a reader that goes
    /// straight to the field gets a number from before the change with nothing to warn it.
    ///
    /// A node with no parent has no siblings, and its position is zero.
    ///
    /// Callable from several threads at once, even though it may write: the numbering is
    /// idempotent and stored atomically, and the epoch that publishes it is stored last.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of this document.
    pub fn ordinal_of(&self, node: NodeIndex) -> u32 {
        let record = self.core(node);
        let Some(parent) = record.parent() else {
            return 0;
        };
        let parent_record = self.core(parent);
        if parent_record.ordinals_valid() != parent_record.ordinals_epoch() {
            crate::node::links::renumber_children(self, parent);
        }
        record.sibling_ordinal()
    }

    /// The columns.
    pub fn columns(&self) -> &Columns {
        &self.columns
    }

    /// The columns, for writing.
    ///
    /// Exclusive, deliberately: a column is written between frames and read during them, and taking
    /// an exclusive borrow to write one is what makes that true rather than hoped for. The
    /// alternative — interior mutability on every column — would drag all twelve of them into the
    /// record's cell discipline for the sake of the handful of stages that write.
    pub fn columns_mut(&mut self) -> &mut Columns {
        &mut self.columns
    }

    /// The class names of `index`.
    pub fn classes_of(&self, index: NodeIndex) -> &[AtomIdent] {
        self.classes.resolve(self.core(index).class_span())
    }

    /// The class pool.
    pub fn class_pool(&self) -> &ClassPool {
        &self.classes
    }

    /// Interns a run of class names and returns the span covering it.
    ///
    /// A run that has been interned before is reused rather than appended again, which is what stops
    /// a state class being toggled on and off for an hour from growing the pool without bound.
    pub fn intern_classes(&mut self, names: &[&str]) -> ClassSpan {
        let atoms: Box<[AtomIdent]> = names.iter().map(|name| AtomIdent::from(*name)).collect();
        if let Some(span) = self.interned_runs.get(&atoms) {
            return *span;
        }
        let span = self.classes.intern(names.iter().copied());
        self.interned_runs.insert(atoms, span);
        span
    }

    /// Replaces `index`'s class names, dropping the cached answer they narrowed.
    ///
    /// The one place classes are written. The narrowed answer to "which interaction-state bits can
    /// matter for this element" is bucketed by the element's own classes, so it stops being an
    /// answer about this element the instant they change; dropping it here, inside the write, is
    /// what stops a state write later in the same batch consulting a mask taken before it.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn write_classes(&mut self, index: NodeIndex, names: &[zgui_interned::ClassName]) {
        let names: Vec<&str> = names.iter().map(|name| name.as_str()).collect();
        let span = self.intern_classes(&names);
        self.core(index).classes.set(span);
        self.invalidate_state_mask(index);
    }

    /// Sets or clears `index`'s identifier, dropping the cached answer it narrowed.
    ///
    /// The one place identifiers are written, and for the same reason as
    /// [`DocumentStore::write_classes`]: the identifier is one of the buckets the answer was
    /// narrowed by.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn write_id(&mut self, index: NodeIndex, id: Option<Ident>) {
        let id = id.map(|id| self.intern_ident(id));
        self.core(index).id_attr.set(id);
        self.invalidate_state_mask(index);
    }

    /// Sets or clears an attribute of `index` other than `id` and `class`.
    ///
    /// The one place attributes are written. Writing one re-asks the installed link resolver whether
    /// this element is a link, in the same call, because that is what makes `:link` and `:visited`
    /// invalidate: the answer is held in the element's interaction state, and the style engine
    /// notices a state change and nothing else. An answer computed during matching instead would
    /// change without the state word changing, so nothing would be invalidated and the previous
    /// style would stay on the screen.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn write_attribute(
        &mut self,
        index: NodeIndex,
        name: zgui_interned::AttrName,
        value: Option<zgui_vocab::SharedString>,
    ) {
        let key = self.key_of(index);
        {
            let slot = self.columns.attrs.get_mut(key);
            match value {
                Some(value) => slot.get_or_insert_with(Box::default).set(name, value),
                None => slot.as_mut().and_then(|attrs| attrs.remove(name)),
            };
            if slot
                .as_deref()
                .is_some_and(crate::side::attrs::AttrMap::is_empty)
            {
                *slot = None;
            }
        }
        self.refresh_link_state(index);
    }

    /// Re-asks the installed link resolver about `index` and folds its answer into its state.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn refresh_link_state(&self, index: NodeIndex) {
        use stylo_dom::ElementState;

        let handle = crate::node::handle::Node::new(self.core(index));
        if !handle.kind().is_element() {
            return;
        }
        let resolver = self.host.links();
        let bit = if !resolver.is_link(handle) {
            ElementState::empty()
        } else if resolver.is_visited(handle) {
            ElementState::VISITED
        } else {
            ElementState::UNVISITED
        };
        let record = self.core(index);
        record.set_state((record.state() - ElementState::VISITED_OR_UNVISITED) | bit);
    }

    /// The identifier table.
    pub fn idents(&self) -> &IdentTable {
        &self.idents
    }

    /// Records `ident` so that it can be resolved to a borrowed atom, and returns it.
    pub fn intern_ident(&mut self, ident: Ident) -> Ident {
        self.idents.intern(ident)
    }

    /// The namespace `id` names.
    ///
    /// # Panics
    ///
    /// Panics if `id` names no entry of this document's namespace table.
    pub fn namespace(&self, id: NamespaceId) -> &web_atoms::Namespace {
        &self.namespaces[id.index() as usize]
    }

    /// Records `uri` in the namespace table if it is not there already, and returns its identifier.
    ///
    /// # Panics
    ///
    /// Panics on the two-hundred-and-fifty-seventh distinct namespace in one document.
    pub fn intern_namespace(&mut self, uri: &str) -> NamespaceId {
        let namespace = web_atoms::Namespace::from(uri);
        if let Some(position) = self.namespaces.iter().position(|held| *held == namespace) {
            return NamespaceId::from_index(position as u8);
        }
        let index = u8::try_from(self.namespaces.len())
            .expect("a document uses at most 256 distinct namespaces");
        self.namespaces.push(namespace);
        NamespaceId::from_index(index)
    }

    /// Stores a record built from the key it is given, and returns its slot number.
    ///
    /// The record holds its own key, which is what lets a handle answer "what is this node called"
    /// without a table lookup, so the slot is chosen first and the record built from it second. A
    /// slot the recycling pass handed back is filled here like any other, and the key table learns
    /// the new occupant's counter — which is what makes every key issued for the slot's previous
    /// occupant stop resolving rather than follow the slot to its next one.
    pub(crate) fn push(&mut self, make: impl FnOnce(NodeKey) -> NodeInner) -> NodeIndex {
        let key = self.arena.insert_with(make);
        let slot = key.index() as usize;
        match self.keys.get_mut(slot) {
            Some(held) => *held = key,
            None => {
                debug_assert_eq!(slot, self.keys.len(), "the arena hands out slots in order");
                self.keys.push(key);
            }
        }
        NodeIndex::new(key.index())
    }

    /// Drops `node`'s record and returns its slot, once the frame that removed it has ended.
    ///
    /// The slot number stays in the key table, holding the key of the occupant that has just gone.
    /// That key no longer resolves, so [`DocumentStore::try_core`] answers [`None`] for the slot
    /// until something is put in it again — which is what lets a record held across the end of a
    /// frame be recognised as stale instead of resolving to whatever moved in.
    pub(crate) fn drop_node(&mut self, node: NodeIndex) {
        let Some(key) = self.keys.get(node.get() as usize).copied() else {
            return;
        };
        self.columns.clear(key);
        self.arena.remove(key);
    }

    /// The node arena, for the recycling pass.
    pub(crate) fn arena_mut(&mut self) -> &mut ChunkArena<NodeInner> {
        &mut self.arena
    }

    /// How many bytes of record, arena slot, key table and column storage the store holds per node.
    ///
    /// Heap held by individual attribute values and text runs is not counted: this is the fixed
    /// cost of having a node at all, which is the number a memory budget is written against.
    pub fn bytes_per_node(&self) -> f64 {
        let nodes = self.len().max(1) as f64;
        let fixed = size_of::<NodeInner>() * self.arena.capacity() as usize
            + size_of::<NodeKey>() * self.keys.capacity()
            + size_of::<AtomIdent>() * self.classes.len();
        fixed as f64 / nodes
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::{Ident, NamespaceId};

    use super::DocumentStore;
    use crate::id::document_id::DocumentId;

    #[test]
    fn a_fresh_store_holds_nothing_and_knows_which_document_it_is() {
        let store = DocumentStore::new(DocumentId::FIRST);
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert_eq!(store.document(), DocumentId::FIRST);
    }

    #[test]
    fn the_null_namespace_is_always_entry_zero() {
        let mut store = DocumentStore::new(DocumentId::FIRST);
        assert_eq!(store.namespace(NamespaceId::NONE).to_string(), "");
        assert_eq!(store.intern_namespace(""), NamespaceId::NONE);

        let svg = store.intern_namespace("http://www.w3.org/2000/svg");
        assert_ne!(svg, NamespaceId::NONE);
        assert_eq!(
            store.namespace(svg).to_string(),
            "http://www.w3.org/2000/svg"
        );
        assert_eq!(store.intern_namespace("http://www.w3.org/2000/svg"), svg);
    }

    #[test]
    fn interning_the_same_class_run_twice_reuses_it() {
        let mut store = DocumentStore::new(DocumentId::FIRST);
        let first = store.intern_classes(&["btn", "large"]);
        let second = store.intern_classes(&["btn", "large"]);
        assert_eq!(first, second);
        assert_eq!(store.class_pool().len(), 2);

        let other = store.intern_classes(&["btn"]);
        assert_ne!(other, first);
        assert_eq!(store.class_pool().len(), 3);
    }

    #[test]
    fn positions_are_numbered_over_the_element_chain_only() {
        use zgui_interned::ElementName;

        use crate::arena::document::Document;
        use crate::node::kind::NodeKind;

        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let first = document.append(root, NodeKind::Element, ElementName::new("item"));
        document.append(root, NodeKind::Text, ElementName::new("#text"));
        let second = document.append(root, NodeKind::Element, ElementName::new("item"));
        document.append(root, NodeKind::Marker, ElementName::new("#marker"));
        let third = document.append(root, NodeKind::Element, ElementName::new("item"));

        let store = document.store();
        assert_eq!(store.ordinal_of(first), 0);
        assert_eq!(store.ordinal_of(second), 1, "the text node has no position");
        assert_eq!(store.ordinal_of(third), 2, "and neither has the marker");
        assert_eq!(store.ordinal_of(root), 0, "the root is its parent's first");
    }

    #[test]
    fn an_insertion_invalidates_the_positions_it_shifted() {
        use zgui_interned::ElementName;

        use crate::arena::document::Document;
        use crate::node::kind::NodeKind;

        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let first = document.append(root, NodeKind::Element, ElementName::new("item"));
        assert_eq!(document.store().ordinal_of(first), 0);

        let second = document.append(root, NodeKind::Element, ElementName::new("item"));
        assert_eq!(document.store().ordinal_of(second), 1);
        assert_eq!(document.store().ordinal_of(first), 0);
    }

    #[test]
    fn an_identifier_resolves_to_a_borrowed_atom() {
        let mut store = DocumentStore::new(DocumentId::FIRST);
        let ident = store.intern_ident(Ident::new("save"));
        assert_eq!(
            store
                .idents()
                .resolve(ident)
                .map(ToString::to_string)
                .as_deref(),
            Some("save")
        );
    }
}
