//! Stable, indented, diffable text: the one place lines and numbers are formatted.
//!
//! A tree dump is read by a person looking at a diff in a review. So it is written through
//! [`Writer`], and every number in it goes through [`number`], because a dump that renders
//! `0.30000001` on one machine and `0.3` on another is not a regression artifact.

pub mod number;
pub mod writer;

pub use crate::text::writer::Writer;
