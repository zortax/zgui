//! Invalidation, at the path its consumers already use.
//!
//! [`Dirty`] and [`DirtyCell`] are pure data structures and live in the crate that holds the
//! framework's bit-twiddling primitives, where their packing is property-tested on its own. They
//! are re-exported here because every consumer of them above this crate already depends on the
//! document and would otherwise have to name a second crate to say what a node owes.
//!
//! [`DirtyChildren`] is not re-exported from anywhere: it names nodes, so it belongs to the crate
//! that has them.
//!
//! | Module | Contents |
//! |---|---|
//! | [`propagate`] | recording that a node owes work, and telling its ancestors |
//! | [`walk`] | visiting the nodes that owe one kind of work, and retiring it |
//! | [`children`] | which of a node's children owe work |

pub mod children;
pub mod propagate;
pub mod walk;

pub use crate::dirty::children::DirtyChildren;
pub use zgui_bits::{Dirty, DirtyCell};
