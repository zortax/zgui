//! The invalidation lattice and the atomic cell that stores it per node.
//!
//! [`Dirty`] names the kinds of work a node can owe. [`DirtyCell`] stores two of them in one
//! machine word — what a node owes itself, and the union of what everything below it owes — so
//! that marking a node and telling its ancestors about it costs one atomic operation per level,
//! and so that a traversal can dismiss a clean subtree by reading a single word.

pub mod bits;
pub mod cell;

pub use crate::dirty::bits::Dirty;
pub use crate::dirty::cell::DirtyCell;
