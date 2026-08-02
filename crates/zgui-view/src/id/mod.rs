//! What a node and a document are called at the seam.
//!
//! Both names are opaque to a view and meaningful to a backend, and both are plain integers so
//! that they cost nothing to store, to copy or to hand to an accessibility tree.

mod document;
mod node;

pub use crate::id::document::{DOCUMENT_COUNT, DocumentId};
pub use crate::id::node::NodeId;
