//! The one-word handle everything is reached through.

use core::hash::{Hash, Hasher};

use crate::arena::store::DocumentStore;
use crate::id::node_key::{NodeIndex, NodeKey, OptIndex};
use crate::node::inner::NodeInner;
use crate::node::kind::NodeKind;

/// A borrowed handle to one node of a document.
///
/// Copying a handle is free and a handle cannot outlive the document it came from. It is exactly
/// one machine word wide, and that is a hard requirement rather than a nicety: the style engine's
/// sharing cache stores an element handle in a word-sized slot and checks the size while it runs,
/// so a two-word handle fails on the first restyle rather than failing to compile.
///
/// Everything else a node has — its attributes, its style, its listeners, its boxes — is reached
/// from here through the store the record points back at, which is what lets the handle stay this
/// small.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Node<'doc>(&'doc NodeInner);

const _: () = assert!(size_of::<Node<'_>>() == size_of::<usize>());
const _: () = assert!(size_of::<Option<Node<'_>>>() == size_of::<usize>());
const _: () = assert!(
    size_of::<usize>() == 8,
    "a node's generation-checked name is packed into a pointer-sized integer"
);

impl<'doc> Node<'doc> {
    /// The handle for a record.
    pub fn new(record: &'doc NodeInner) -> Self {
        Self(record)
    }

    /// The record behind this handle.
    pub fn record(self) -> &'doc NodeInner {
        self.0
    }

    /// The store that owns this node.
    pub fn store(self) -> &'doc DocumentStore {
        // SAFETY: the handle borrows the document for `'doc`, so the store outlives `'doc`.
        unsafe { self.0.store() }
    }

    /// This node's generation-checked name.
    pub fn key(self) -> NodeKey {
        self.0.key()
    }

    /// This node's slot number.
    pub fn index(self) -> NodeIndex {
        self.0.index()
    }

    /// What this node is.
    pub fn kind(self) -> NodeKind {
        self.0.kind()
    }

    /// The handle for another node of the same document.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of this document.
    pub fn sibling(self, index: NodeIndex) -> Self {
        Self(self.store().core(index))
    }

    /// The handle an optional slot number names.
    pub fn opt(self, index: OptIndex) -> Option<Self> {
        index.get().map(|index| self.sibling(index))
    }
}

impl core::fmt::Debug for Node<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_tuple("Node")
            .field(&self.index().get())
            .finish()
    }
}

impl PartialEq for Node<'_> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.0, other.0)
    }
}

impl Eq for Node<'_> {}

impl Hash for Node<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(core::ptr::from_ref(self.0) as usize);
    }
}
