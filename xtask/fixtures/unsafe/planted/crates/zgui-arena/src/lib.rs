//! An allowlisted crate.

/// A node the parallel traversal reads through a shared reference.
pub struct NodeInner;

// PLANTED VIOLATION: a Sync promise with no reason stated above it.
unsafe impl Sync for NodeInner {}
