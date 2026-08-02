//! Driving a text engine, and a paint table, from a layout pass.
//!
//! Laying text out needs three things a layout pass cannot own: a shaper, somewhere to keep what it
//! shaped, and a table of brushes. [`Paragraphs`] is the three of them wired together as the one
//! thing a pass does need — something that can answer how big a piece of content is.

pub mod paragraphs;
pub mod reshape;

pub use crate::text::paragraphs::Paragraphs;
