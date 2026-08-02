//! Changing part of a box tree in place instead of building a new one.
//!
//! Building a box tree replaces every box in it, and a box's name is what fragment reuse, geometry
//! diffing, the per-fragment paint record and damage scissoring are all keyed on. So a frame that
//! rebuilds is a frame in which none of those can hit: every box is new, every fragment is new,
//! every fragment compares as changed, and the damage grows to the root's ink — the whole window,
//! whatever actually moved. Rebuilding is therefore the fallback, and the operations here are what
//! a frame reaches for first.
//!
//! | Module | What it changes without rebuilding the tree |
//! |---|---|
//! | [`subtree`] | the boxes one element generates, spliced in where its old ones were |
//! | [`structure`] | which boxes are in the tree, for a change that adds or removes one |
//! | [`style`] | the computed style a kept box is laid out and painted with |
//! | [`text`] | the characters a kept text-run box lays out |
//!
//! # What a patch owes the rest of the frame
//!
//! Three things, and leaving out any one of them is a defect with no error and no log.
//!
//! A patch that changes what a box *is made of* has to throw away the layout of that box and of
//! every ancestor, or the sizes computed against the old content stand. A patch that changes the
//! characters in an inline formatting context has to drop that context's flattened form, because
//! the flattened form is checked against the sequence of boxes it was built from and a box that was
//! rewritten in place is the same box. And a patch that cannot express what changed has to say so,
//! so that the caller rebuilds rather than laying out a tree that no longer describes the document.

pub mod structure;
pub mod style;
pub mod subtree;
pub mod text;

pub use crate::boxtree::patch::structure::{detach, replace};
pub use crate::boxtree::patch::style::restyle;
pub use crate::boxtree::patch::subtree::{Rebuilt, rebuild};
pub use crate::boxtree::patch::text::{Retext, retext};
