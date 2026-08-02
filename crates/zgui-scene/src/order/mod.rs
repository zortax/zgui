//! Draw-order assignment.
//!
//! One question, asked once per primitive: *what is the lowest order that still draws this above
//! everything it overlaps?* Answering it with a spatial index rather than a running counter is what
//! lets disjoint content share an order and batch together, and it is what makes the invariant the
//! rest of the crate leans on true — **two primitives at equal order are provably
//! non-overlapping**.

pub mod sweep;
pub mod tree;

#[cfg(test)]
mod tests;

pub use crate::order::tree::BoundsTree;
