//! An allowlisted crate: chunk initialisation hands out address-stable references.

/// A node the parallel traversal reads through a shared reference.
pub struct NodeInner;

// SAFETY: every field is a Cell of a Copy type or an atomic, so the reads the traversal
// performs are shared reads of memory nobody writes.
unsafe impl Sync for NodeInner {}
