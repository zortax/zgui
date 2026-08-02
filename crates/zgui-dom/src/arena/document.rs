//! The document: an owned store, and the operations that build one.

use core::cell::Cell;
use std::sync::Arc;

use zgui_arena::DocumentId;
use zgui_interned::{AttrName, ClassName, ElementName as InternedName, Ident, NamespaceId};
use zgui_vocab::SharedString;

use crate::arena::store::DocumentStore;
use crate::host::{Intrinsic, LinkResolver, PresentationalHints, ReplacedContent, SheetLoader};
use crate::id::node_key::{NodeIndex, OptIndex};
use crate::mutate::edit::session::EditState;
use crate::node::element::name::ElementName;
use crate::node::flags::NodeFlags;
use crate::node::handle::Node;
use crate::node::inner::NodeInner;
use crate::node::kind::NodeKind;
use crate::node::links;

/// One document.
///
/// The store is heap-allocated and never moves, because every record in it holds a pointer back to
/// it; the document owns that allocation and hands out shared access to it. Moving a document moves
/// the pointer and not the allocation, so no record's back-pointer changes.
///
/// # Building versus editing
///
/// The methods here are the **construction** path: they link a node into the tree and write its
/// name, classes, identifier and state directly. They take no snapshot of what an element looked
/// like before, and they mark nothing as needing work, because a document that is being built has
/// no computed styles to invalidate and nothing downstream that has seen it yet.
///
/// Changing a document that is already live is a different operation with a different contract —
/// what changed has to be recorded before it is overwritten, and the ancestors have to learn that
/// there is work below them — and it goes through the batched editing API rather than through these.
/// Both maintain the two child chains through the same code, so the chains cannot disagree.
pub struct Document {
    /// The store. Owned here, freed in [`Document::drop`].
    store: *mut DocumentStore,
    /// The root element, once one has been added.
    ///
    /// A cell because the batched editing API changes a document through a shared reference, and
    /// removing the root element is one of the changes it can make.
    root: Cell<OptIndex>,
    /// Everything the batched editing API needs to change a document through a shared reference.
    pub(crate) edit: EditState,
}

// SAFETY: the pointer is the sole owner of a store, which is itself `Sync`; sending a document
// moves the pointer and not the allocation, so no record's back-pointer changes and no reference
// into the store is invalidated.
unsafe impl Send for Document {}
// SAFETY: shared access to a document yields shared access to a `Sync` store, and to the editing
// state, whose scratch is written through a shared reference. That scratch has one writer at a
// time: opening a batch takes a single-writer token first, a second thread's attempt fails rather
// than proceeding, and the token is released and re-acquired with the orderings that put one
// thread's batch entirely before the next thread's. The root cell is written only from inside a
// batch, so it is covered by the same token.
unsafe impl Sync for Document {}

impl Document {
    /// An empty document, with its document node at slot zero.
    pub fn new() -> Self {
        Self::with_id(DocumentId::FIRST)
    }

    /// An empty document identified as `document`.
    ///
    /// Two windows are two documents, and giving them distinct identities is what stops a node name
    /// from one ever resolving inside the other.
    pub fn with_id(document: DocumentId) -> Self {
        let store = Box::into_raw(Box::new(DocumentStore::new(document)));
        let mut built = Self {
            store,
            root: Cell::new(OptIndex::NONE),
            edit: EditState::new(),
        };
        let node = built.store_mut().push(|key| {
            NodeInner::new(
                store,
                key,
                NodeKind::Document,
                ElementName::new(InternedName::new("#document")),
            )
        });
        built.store().core(node).set_flags(NodeFlags::IN_DOCUMENT);
        debug_assert_eq!(node, NodeIndex::new(0));
        built
    }

    /// The store, shared.
    pub fn store(&self) -> &DocumentStore {
        // SAFETY: the pointer came from `Box::into_raw` in the constructor and is freed only in
        // `Drop`, so it is live for as long as the document is.
        unsafe { &*self.store }
    }

    /// The store, exclusively.
    pub fn store_mut(&mut self) -> &mut DocumentStore {
        // SAFETY: as `store`, and the exclusive borrow of the document rules out any other live
        // borrow of the store.
        unsafe { &mut *self.store }
    }

    /// The address of the store, which every record holds as its back-pointer.
    pub(crate) fn store_ptr(&self) -> *mut DocumentStore {
        self.store
    }

    /// The store, exclusively, from a shared borrow of the document.
    ///
    /// This is what lets the batched editing API take a shared reference, which every method a
    /// view calls does.
    ///
    /// # Safety
    ///
    /// The caller must hold the document's single-writer token — that is, must be inside an open
    /// batch — and must not hold any other reference into the store for the life of the returned
    /// one. The style traversal is the only other reader of the store, and it may not run while a
    /// batch is open.
    // Handing an exclusive reference out of a shared one is the whole point: every method a view
    // calls takes a shared document, and the single-writer token is what makes it sound.
    #[allow(
        clippy::mut_from_ref,
        reason = "the caller's obligation is stated above"
    )]
    pub(crate) unsafe fn store_exclusive(&self) -> &mut DocumentStore {
        // SAFETY: the pointer came from `Box::into_raw` in the constructor and is freed only in
        // `Drop`; exclusivity is the caller's obligation, stated above.
        unsafe { &mut *self.store }
    }

    /// A handle to the node at `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn node(&self, index: NodeIndex) -> Node<'_> {
        Node::new(self.store().core(index))
    }

    /// The document node's slot number, which is always zero.
    pub fn document_index(&self) -> NodeIndex {
        NodeIndex::new(0)
    }

    /// A handle to the document node.
    pub fn document_node(&self) -> Node<'_> {
        self.node(self.document_index())
    }

    /// The root element, if the document has one.
    pub fn root(&self) -> Option<Node<'_>> {
        self.root_index().map(|index| self.node(index))
    }

    /// The root element's slot number, if the document has a root.
    pub fn root_index(&self) -> Option<NodeIndex> {
        self.root.get().get()
    }

    /// Records which element is the document's root, or that it has none.
    pub(crate) fn set_root_index(&self, root: Option<NodeIndex>) {
        self.root.set(OptIndex::from_option(root));
    }

    /// How many nodes the document holds, its document node included.
    pub fn len(&self) -> usize {
        self.store().len()
    }

    /// Whether the document holds nothing but its document node.
    pub fn is_empty(&self) -> bool {
        self.store().len() <= 1
    }

    /// Appends a node of `kind` and name `name`, in no namespace, to `parent`.
    ///
    /// # Panics
    ///
    /// Panics if `parent` names no live node of this document.
    pub fn append(&mut self, parent: NodeIndex, kind: NodeKind, name: InternedName) -> NodeIndex {
        self.append_in(parent, kind, name, NamespaceId::NONE)
    }

    /// Appends a node of `kind` and name `name`, in `namespace`, to `parent`.
    ///
    /// A node's namespace is written when the record is created and never afterwards, because a
    /// name and the namespace that qualifies it are one fact and changing half of it is not an
    /// operation any document language offers. Interning the URI first is what turns it into an
    /// identifier this can take.
    ///
    /// # Panics
    ///
    /// Panics if `parent` names no live node of this document.
    pub fn append_in(
        &mut self,
        parent: NodeIndex,
        kind: NodeKind,
        name: InternedName,
        namespace: NamespaceId,
    ) -> NodeIndex {
        let store: *mut DocumentStore = self.store;
        let local_name = ElementName::new(name);
        let index = self.store_mut().push(|key| {
            let mut record = NodeInner::new(store, key, kind, local_name);
            record.namespace = namespace;
            record
        });

        links::append_child(self.store(), parent, index);
        self.store().core(index).set_flags(NodeFlags::IN_DOCUMENT);
        crate::mutate::edit::build::note_root(self, index, parent);
        index
    }

    /// Creates a node of `kind` and name `name` that is not linked into the tree.
    ///
    /// A detached node is the raw material of an insertion: it is built, given its content, and
    /// then linked in as one subtree. Nothing reaches it until it is, so nothing about it needs
    /// recording while it is being built.
    pub fn detached(&mut self, kind: NodeKind, name: InternedName) -> NodeIndex {
        let store: *mut DocumentStore = self.store;
        let local_name = ElementName::new(name);
        self.store_mut()
            .push(|key| NodeInner::new(store, key, kind, local_name))
    }

    /// Replaces `index`'s classes.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn set_classes(&mut self, index: NodeIndex, names: &[ClassName]) {
        self.store_mut().write_classes(index, names);
    }

    /// Sets or clears `index`'s `id`.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn set_id(&mut self, index: NodeIndex, id: Option<Ident>) {
        self.store_mut().write_id(index, id);
    }

    /// Sets or clears an attribute other than `id` and `class` on `index`.
    ///
    /// Those two are not attributes here: they live in the node record, as a copyable identifier
    /// handle and as a span into the document's class pool, and they are written through
    /// [`Document::set_id`] and [`Document::set_classes`].
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn set_attribute(&mut self, index: NodeIndex, name: AttrName, value: Option<SharedString>) {
        self.store_mut().write_attribute(index, name, value);
    }

    /// Re-asks the installed link resolver about `index` and folds its answer into the node's state.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn refresh_link_state(&mut self, index: NodeIndex) {
        self.store().refresh_link_state(index);
    }

    /// `index`'s namespace.
    pub fn namespace_of(&self, index: NodeIndex) -> NamespaceId {
        self.store().core(index).namespace_id()
    }

    /// Replaces `index`'s interaction state.
    ///
    /// This is the construction path, which records nothing about what the state was before.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn set_state(&mut self, index: NodeIndex, state: zgui_vocab::UiState) {
        self.store()
            .core(index)
            .set_state(crate::node::element::state::to_engine(state));
    }

    /// Sets `index`'s structural flags.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn set_flags(&mut self, index: NodeIndex, flags: NodeFlags) {
        self.store().core(index).set_flags(flags);
    }

    /// Installs the source of declarations derived from markup attributes.
    pub fn install_presentational_hints(&mut self, hints: Arc<dyn PresentationalHints>) {
        self.store_mut().host_mut().set_hints(hints);
    }

    /// Installs the link resolver, and re-asks it about every element already in the document.
    ///
    /// The refresh is the point: the resolver's answer is stored on each element rather than asked
    /// for during matching, so installing one after the document exists would otherwise leave every
    /// existing element reporting that it is not a link.
    pub fn install_link_resolver(&mut self, links: Arc<dyn LinkResolver>) {
        self.store_mut().host_mut().set_links(links);
        for slot in 0..self.store().slot_count() as u32 {
            let index = NodeIndex::new(slot);
            if self.store().try_core(index).is_some() {
                self.refresh_link_state(index);
            }
        }
    }

    /// Installs the source of intrinsic sizing for replaced nodes.
    pub fn install_replaced_content(&mut self, replaced: Arc<dyn ReplacedContent>) {
        self.store_mut().host_mut().set_replaced(replaced);
    }

    /// Installs the stylesheet loader `@import` resolves through.
    pub fn install_sheet_loader(&mut self, sheets: Arc<dyn SheetLoader>) {
        self.store_mut().host_mut().set_sheets(sheets);
    }

    /// The natural size, ratio and baseline of `index`'s content, if it is a replaced node.
    ///
    /// [`None`] means the node is not replaced at all, which is a different answer from a replaced
    /// node whose content has no intrinsic size yet.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn intrinsic_of(&self, index: NodeIndex) -> Option<Intrinsic> {
        let id = self.node(index).replaced_id()?;
        Some(self.store().host().replaced().intrinsic(id))
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `Box::into_raw` in the constructor, is not shared with any
        // other owner, and no handle can outlive the document that hands them out.
        drop(unsafe { Box::from_raw(self.store) });
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::{ClassName, ElementName, Ident};
    use zgui_vocab::UiState;

    use super::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn a_fresh_document_holds_only_its_document_node() {
        let document = Document::new();
        assert!(document.is_empty());
        assert_eq!(document.len(), 1);
        assert_eq!(document.document_node().kind(), NodeKind::Document);
        assert!(document.root().is_none());
    }

    #[test]
    fn the_first_element_child_of_the_document_becomes_the_root() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        assert_eq!(document.root_index(), Some(root));
        assert!(
            document
                .store()
                .core(root)
                .has_flags(crate::node::flags::NodeFlags::IS_ROOT)
        );

        let second = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("other"),
        );
        assert_eq!(document.root_index(), Some(root), "the root is chosen once");
        assert!(
            !document
                .store()
                .core(second)
                .has_flags(crate::node::flags::NodeFlags::IS_ROOT)
        );
    }

    #[test]
    fn classes_identifiers_and_state_round_trip() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        document.set_classes(root, &[ClassName::new("btn"), ClassName::new("large")]);
        document.set_id(root, Some(Ident::new("save")));
        document.set_state(root, UiState::HOVER | UiState::ENABLED);

        assert_eq!(document.store().classes_of(root).len(), 2);
        assert_eq!(document.store().classes_of(root)[0].to_string(), "btn");
        let id = document
            .store()
            .core(root)
            .id_attr()
            .expect("an id was set");
        assert_eq!(
            document
                .store()
                .idents()
                .resolve(id)
                .map(ToString::to_string)
                .as_deref(),
            Some("save")
        );
        assert_eq!(
            crate::node::element::state::from_engine(document.store().core(root).state()),
            UiState::HOVER | UiState::ENABLED
        );
    }

    #[test]
    fn a_namespace_is_recorded_on_the_node_and_resolvable_from_the_store() {
        let mut document = Document::new();
        let namespace = document
            .store_mut()
            .intern_namespace("http://www.w3.org/2000/svg");
        let root = document.append_in(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("svg"),
            namespace,
        );
        let id = document.namespace_of(root);
        assert_eq!(
            document.store().namespace(id).to_string(),
            "http://www.w3.org/2000/svg"
        );
    }

    #[test]
    fn a_document_can_be_sent_between_threads_without_moving_its_store() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let held = core::ptr::from_ref(document.store().core(root)) as usize;
        let moved = std::thread::spawn(move || {
            (
                core::ptr::from_ref(document.store().core(root)) as usize,
                document,
            )
        })
        .join()
        .expect("the thread ran");
        assert_eq!(held, moved.0);
    }
}
