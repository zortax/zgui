//! Stable, indented, diffable text: the one place lines and numbers are formatted.
//!
//! Two very different things are printed by this crate — a finished display list and a tree — and
//! both are read the same way: by a person looking at a diff in a review. So both are written
//! through [`Writer`], and every number in either goes through [`number`], because a transcript
//! that renders `0.30000001` on one machine and `0.3` on another is not a regression artifact.

pub mod number;
pub mod writer;

pub use crate::text::writer::Writer;
