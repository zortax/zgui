//! The two child chains, and the one place that keeps them agreeing.
//!
//! A node has children in two senses and they are different lists. The **plain** chain holds
//! everything — elements, text, markers — and is what the style traversal walks. The
//! **element-only** chain skips everything that is not an element, and is what selector matching
//! walks: `+`, `~`, `:first-child` and `:nth-child` all step along it, on the hot path, once per
//! candidate. Deriving the second from the first at match time turns a constant-time step into a
//! scan past however much text happens to be in the way, so both are maintained at insertion.
//!
//! Both chains are maintained here and nowhere else, because they are only correct together.
//!
//! Linking a node in and taking one out are one submodule each, because the two are mirrors and the
//! element-only chain is what they get wrong: appending is the common case, so an append-only
//! implementation is the one that gets exercised, and the insertion path is then subtly wrong in the
//! presence of text nodes.

pub(crate) mod insert;
pub(crate) mod remove;

use core::sync::atomic::Ordering;

pub(crate) use crate::node::links::insert::{append_child, link_before};
pub(crate) use crate::node::links::remove::unlink;

use crate::arena::store::DocumentStore;
use crate::id::node_key::{NodeIndex, OptIndex};
use crate::node::inner::NodeInner;

impl NodeInner {
    /// This node's parent.
    pub fn parent(&self) -> Option<NodeIndex> {
        self.parent.get().get()
    }

    /// This node's first child of any kind.
    pub fn first_child(&self) -> Option<NodeIndex> {
        self.first_child.get().get()
    }

    /// This node's last child of any kind.
    pub fn last_child(&self) -> Option<NodeIndex> {
        self.last_child.get().get()
    }

    /// This node's previous sibling of any kind.
    pub fn prev_sibling(&self) -> Option<NodeIndex> {
        self.prev_sibling.get().get()
    }

    /// This node's next sibling of any kind.
    pub fn next_sibling(&self) -> Option<NodeIndex> {
        self.next_sibling.get().get()
    }

    /// This node's previous element sibling, skipping text and markers.
    pub fn prev_element(&self) -> Option<NodeIndex> {
        self.prev_element.get().get()
    }

    /// This node's next element sibling, skipping text and markers.
    pub fn next_element(&self) -> Option<NodeIndex> {
        self.next_element.get().get()
    }

    /// This node's first element child, skipping text and markers.
    pub fn first_element_child(&self) -> Option<NodeIndex> {
        self.first_element_child.get().get()
    }

    /// How many children of any kind this node has.
    pub fn child_count(&self) -> u32 {
        self.child_count.get()
    }

    /// Whether this node has no children of any kind.
    pub fn has_no_children(&self) -> bool {
        self.first_child.get().is_none()
    }

    /// The epoch at which a structural change under this node was last recorded.
    pub(crate) fn ordinals_epoch(&self) -> u32 {
        self.ordinals_epoch.get()
    }

    /// The epoch this node's children's positions were last numbered at.
    ///
    /// Acquired, because it is what publishes the numbering: a reader that finds it current is
    /// guaranteed to see every position the numbering stored before it.
    pub(crate) fn ordinals_valid(&self) -> u32 {
        self.ordinals_valid.load(Ordering::Acquire)
    }

    /// This node's recorded position among its element siblings, which may be stale.
    ///
    /// Read through the store rather than here, so that a stale number is renumbered before it is
    /// believed.
    pub(crate) fn sibling_ordinal(&self) -> u32 {
        self.sibling_ordinal.load(Ordering::Relaxed)
    }
}

/// Numbers `parent`'s element children from zero, and records the epoch it did it at.
///
/// Numbering is lazy because doing it eagerly makes building an *n*-item list quadratic: appending
/// a thousand rows would renumber a thousand times. Instead a structural change bumps the parent's
/// epoch and the first read afterwards pays for one pass.
///
/// # Concurrency
///
/// Taken on a shared borrow, so two readers can be running this over the same child list at the
/// same moment. That is safe because it is *idempotent*: both compute the same positions from the
/// same chain and store the same numbers, every store is atomic, and the epoch that says the
/// numbering is current is stored last, so nobody can observe a half-numbered list as current.
///
/// # Panics
///
/// Panics if `parent` names no live node of `store`.
pub(crate) fn renumber_children(store: &DocumentStore, parent: NodeIndex) {
    let parent_record = store.core(parent);
    let mut ordinal = 0;
    let mut current = parent_record.first_element_child();
    while let Some(index) = current {
        let record = store.core(index);
        record.sibling_ordinal.store(ordinal, Ordering::Relaxed);
        ordinal += 1;
        current = record.next_element();
    }
    parent_record
        .ordinals_valid
        .store(parent_record.ordinals_epoch(), Ordering::Release);
}

/// The nearest element at or before `from` on the plain sibling chain.
pub(crate) fn previous_element(store: &DocumentStore, from: OptIndex) -> Option<NodeIndex> {
    let mut scan = from.get();
    while let Some(candidate) = scan {
        let record = store.core(candidate);
        if record.kind().in_element_chain() {
            return Some(candidate);
        }
        scan = record.prev_sibling.get().get();
    }
    None
}

/// The nearest element at or after `from` on the plain sibling chain.
pub(crate) fn following_element(store: &DocumentStore, from: OptIndex) -> Option<NodeIndex> {
    let mut scan = from.get();
    while let Some(candidate) = scan {
        let record = store.core(candidate);
        if record.kind().in_element_chain() {
            return Some(candidate);
        }
        scan = record.next_sibling.get().get();
    }
    None
}

/// `parent`'s last element child, skipping text and markers.
///
/// Derived rather than stored: the walk is over whatever non-elements trail the child list, which
/// is nothing at all in every shape a document actually takes, and a stored field would be four
/// more bytes on every node in the document to save it.
///
/// # Panics
///
/// Panics if `parent` names no live node of `store`.
pub(crate) fn last_element_child(store: &DocumentStore, parent: NodeIndex) -> Option<NodeIndex> {
    previous_element(store, store.core(parent).last_child.get())
}

/// Records that `parent`'s child list changed shape, so its positions are renumbered on next read.
///
/// # Panics
///
/// Panics if `parent` names no live node of `store`.
pub(crate) fn note_shape_change(store: &DocumentStore, parent: NodeIndex) {
    let record = store.core(parent);
    record
        .ordinals_epoch
        .set(record.ordinals_epoch.get().wrapping_add(1));
}
