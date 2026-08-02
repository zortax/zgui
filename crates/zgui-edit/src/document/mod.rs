//! Putting an editor's paragraphs into a document, one text node each.
//!
//! An editable element holds one text node per paragraph rather than one node holding every line,
//! and that is the whole of what makes a keystroke cheap. A shaper has no incremental mode: it
//! shapes a paragraph at a time and caches the result by content, so a node whose text is
//! unchanged keeps its shaped result and a node whose text changed re-shapes alone. One node
//! holding the whole document would change on every keystroke and re-shape all of it.
//!
//! The break between two paragraphs is written into the node of the paragraph it ends, so the text
//! under the element read in order is the text the editor holds — and so that a line break is a
//! character the layout can break a line at. See [`content_of`].

pub mod projection;

pub use crate::document::projection::{Projection, content_of};
