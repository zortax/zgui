//! Creating nodes, and linking them into and out of the tree.
//!
//! These are the changes whose consequences reach furthest, and the only ones whose consequences
//! are not about the node that changed. A child appearing or disappearing changes what
//! `:nth-child`, `:last-of-type`, `:empty` and every `+` and `~` combinator match on the *other*
//! children of the same parent — elements nothing touched.
//!
//! Each of the two has one obligation the other lacks, and each is easy to miss for its own reason.
//!
//! **An insertion has to splice.** The subtree was built while detached, so everything marked on it
//! stopped at its own root — there were no ancestors to tell. Marking that root again does not
//! repair it, because marking returns at the node's own word before reaching an ancestor. So what
//! the subtree already owes is folded into its new ancestors explicitly.
//!
//! **A removal has to hand its subtree over before it goes.** The area a removed subtree occupied
//! is nobody's to repaint afterwards: the stage that works out what changed compares output that
//! still exists, and the removed output does not, so the vacated area is never anyone's previous
//! extent and the pixels stay on the screen for ever. So the removed root is recorded — its boxes
//! are still readable, because a removed node's columns survive until the frame's recycling pass —
//! and its parent is marked for layout and paint directly. That mark is the one damage no restyle
//! can produce, which is why it is the single exception to damage coming from the style engine.

use style::invalidation::element::restyle_hints::RestyleHint;
use zgui_bits::Dirty;
use zgui_interned::ElementName as InternedName;

use crate::arena::document::Document;
use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;
use crate::mutate::ancestors;
use crate::mutate::edit::Edit;
use crate::node::element::name::ElementName;
use crate::node::flags::NodeFlags;
use crate::node::inner::NodeInner;
use crate::node::kind::NodeKind;
use crate::node::links;

impl Edit<'_> {
    /// Creates a detached element.
    ///
    /// A detached node is the raw material of an insertion: nothing reaches it until it is linked
    /// in, so nothing about building it has to be recorded.
    pub fn create_element(&mut self, name: InternedName) -> NodeIndex {
        self.create(NodeKind::Element, name)
    }

    /// Creates a detached element whose content comes from outside the document.
    ///
    /// The replaced flag is part of creation rather than a later write because box building keys
    /// off it: a node born replaced never has a box built from the wrong classification, while a
    /// node that *became* replaced would owe a rebuild of the box it already has. The flag says
    /// only where the content comes from; what the content *is* arrives through the installed
    /// [`ReplacedContent`](crate::host::replaced::ReplacedContent) source, possibly much later.
    pub fn create_replaced_element(&mut self, name: InternedName) -> NodeIndex {
        let node = self.create(NodeKind::Element, name);
        set_flag(self.store(), node, NodeFlags::IS_REPLACED, true);
        node
    }

    /// Creates a detached text node holding `text`.
    pub fn create_text(&mut self, text: &str) -> NodeIndex {
        let node = self.create(NodeKind::Text, InternedName::new("#text"));
        crate::text::node::set_text(self.store(), node, text);
        node
    }

    /// Creates a detached marker: a node that holds a place in a child list and generates nothing.
    pub fn create_marker(&mut self) -> NodeIndex {
        self.create(NodeKind::Marker, InternedName::new("#marker"))
    }

    /// Creates a detached node of `kind` called `name`.
    fn create(&mut self, kind: NodeKind, name: InternedName) -> NodeIndex {
        let store_ptr = self.document().store_ptr();
        let local_name = ElementName::new(name);
        self.store()
            .push(|key| NodeInner::new(store_ptr, key, kind, local_name))
    }

    /// Links `child` into `parent`'s children, ahead of `before` or at the end if there is none.
    ///
    /// `child` may be freshly created or may already be somewhere in the document, in which case it
    /// is moved: its subtree travels with it, and because it now inherits from somewhere else,
    /// every computed value below it that inherits is recomputed — without re-running selector
    /// matching, which cannot have changed for a subtree that moved whole.
    ///
    /// # Panics
    ///
    /// Panics if any index names no live node of the document, or if `before` is not a child of
    /// `parent`.
    pub fn insert_before(
        &mut self,
        parent: NodeIndex,
        child: NodeIndex,
        before: Option<NodeIndex>,
    ) {
        let moved = self.store().core(child).parent().is_some();
        if moved {
            self.take_out(child);
        }

        let (store, batch) = self.parts();
        // Recorded before the links move: the earliest child an insertion can have changed the
        // match of is the inserted node itself — when it is an element. A text node or a marker has
        // no position among element siblings, so it moves none of them and only the parent's own
        // emptiness can have changed.
        if store.core(child).kind().in_element_chain() {
            batch.structure.record_change(store, parent, Some(child));
        } else {
            batch.structure.record_emptiness_change(store, parent);
        }

        let was_root = is_document_child(store, child);
        links::link_before(store, parent, child, before);
        set_flag(store, child, NodeFlags::IN_DOCUMENT, true);
        if was_root != is_document_child(store, child) {
            // The cached answer to "which interaction-state bits can matter here" is narrowed by
            // whether the element is the document's root, so a move across that boundary is one of
            // the writes that must drop it.
            store.invalidate_state_mask(child);
        }

        ancestors::mark(store, parent, Dirty::CHILDREN);
        ancestors::mark(store, child, Dirty::RESTYLE | Dirty::A11Y);
        if moved {
            batch
                .hints
                .record(store, child, RestyleHint::RECASCADE_DESCENDANTS);
        }
        // What the subtree already owed reaches its new ancestors only here.
        ancestors::splice(store, child);
        note_root(self.document(), child, parent);
    }

    /// Takes `node` and everything below it out of the document.
    ///
    /// Does nothing if `node` is already detached.
    ///
    /// The subtree's records stay readable for the rest of the frame — the accessibility walk over
    /// its boxes and the stage that works out what changed both still hold names into it — and are
    /// dropped by [`end_frame`](crate::arena::end_frame). Linking the subtree in again before then
    /// keeps it: what the frame's end drops is what is still detached when it runs.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn remove(&mut self, node: NodeIndex) {
        let Some(parent) = self.store().core(node).parent() else {
            return;
        };
        self.take_out(node);
        let (store, batch) = self.parts();
        batch.removed.push(node);
        batch.detached.push(node);
        // The parent, and not the node that left: a removal is the one change whose only surviving
        // witness is the parent. An accessibility tree takes a child list from the parent alone, so
        // without `A11Y` here the consumer keeps announcing a control that is no longer on the
        // screen — and every identifier below it stays live in a tree nothing points at.
        ancestors::mark(
            store,
            parent,
            Dirty::CHILDREN | Dirty::RELAYOUT | Dirty::REPAINT | Dirty::A11Y,
        );
        note_root_removed(self.document(), node, parent);
    }

    /// The child-list record and the link surgery a removal and a move both do.
    fn take_out(&mut self, node: NodeIndex) {
        let (store, batch) = self.parts();
        let Some(parent) = store.core(node).parent() else {
            return;
        };
        // The earliest child a removal can have changed the match of is the element that followed
        // it — nothing at all when it was the last, because there is then no later sibling whose
        // position or combinator match could have changed. A node that is not an element occupied
        // no position, so taking it out moves none: only the parent's own emptiness can have
        // changed, and recording the rest would restyle the whole child list for nothing.
        if store.core(node).kind().in_element_chain() {
            let anchor = store.core(node).next_element();
            batch.structure.record_change(store, parent, anchor);
        } else {
            batch.structure.record_emptiness_change(store, parent);
        }
        let was_root = is_document_child(store, node);

        links::unlink(store, node);
        set_flag(store, node, NodeFlags::IN_DOCUMENT, false);
        if was_root {
            set_flag(store, node, NodeFlags::IS_ROOT, false);
            store.invalidate_state_mask(node);
        }
    }
}

/// Sets or clears one structural flag on `node`.
fn set_flag(store: &DocumentStore, node: NodeIndex, flag: NodeFlags, on: bool) {
    let record = store.core(node);
    let flags = record.flags();
    record.set_flags(if on { flags | flag } else { flags - flag });
}

/// Whether `node`'s parent is the document node, which is what makes an element the document root.
fn is_document_child(store: &DocumentStore, node: NodeIndex) -> bool {
    store
        .core(node)
        .parent()
        .is_some_and(|parent| parent == NodeIndex::new(0))
}

/// Records that `node` has become the document's root, if it has.
pub(crate) fn note_root(document: &Document, node: NodeIndex, parent: NodeIndex) {
    if parent != document.document_index()
        || !document.store().core(node).kind().is_element()
        || document.root_index().is_some()
    {
        return;
    }
    document.set_root_index(Some(node));
    let record = document.store().core(node);
    record.set_flags(record.flags() | NodeFlags::IS_ROOT);
}

/// Records that the document's root has left, and promotes whatever is first in its place.
fn note_root_removed(document: &Document, node: NodeIndex, parent: NodeIndex) {
    if document.root_index() != Some(node) {
        return;
    }
    document.set_root_index(None);
    if let Some(next) = document.store().core(parent).first_element_child() {
        note_root(document, next, parent);
    }
}
