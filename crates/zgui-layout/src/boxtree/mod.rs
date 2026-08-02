//! Turning a styled document into the boxes CSS says it generates.
//!
//! An element and a box are not one-to-one and CSS forces that in several separate ways: a hidden
//! element generates none, an element whose children reparent onto its own parent generates none, a
//! run of inline siblings shares one anonymous box, generated content produces boxes with no
//! element at all, `order` moves a box without moving its element, and an absolutely positioned box
//! is laid out by an ancestor that is not the one it was written inside.
//!
//! Every one of those is settled here, once, while the tree is built — never while it is walked,
//! because the walk runs inside the layout algorithms' innermost loops.

pub mod absolute;
pub mod anonymous;
pub mod build;
pub mod classify;
pub mod contents;
pub mod order;
pub mod patch;
pub mod pseudo;

pub use crate::boxtree::build::{Owed, REBUILDS, build, retire};
