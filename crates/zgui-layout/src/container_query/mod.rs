//! Laying out again when a container's own size changed the styles inside it.
//!
//! A container query is a cycle by construction: the styles inside a container depend on its
//! resolved size, and its resolved size depends on those styles. There is no ordering that resolves
//! it in one pass, so it is resolved by iteration to a fixed point, with a cap — a document that has
//! not settled after three attempts is one whose queries contradict each other, and stopping with
//! the third answer is better than not stopping.

pub mod fixpoint;

pub use crate::container_query::fixpoint::{Converged, MAX_PASSES, run};
