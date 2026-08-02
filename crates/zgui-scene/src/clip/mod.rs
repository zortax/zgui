//! Clipping: a chain of links, and the flattened form a draw call binds.

pub mod link;
pub mod path;
pub mod resolved;
pub mod table;

#[cfg(test)]
mod tests;

pub use crate::clip::link::{ClipLink, ClipNode, MaskSource};
pub use crate::clip::resolved::{ResolvedClip, RoundedTest};
pub use crate::clip::table::ClipTable;
