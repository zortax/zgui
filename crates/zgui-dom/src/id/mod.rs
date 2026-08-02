//! What a node is called, at each of the three ranges a name has to travel.
//!
//! * [`NodeIndex`] is a slot number inside one document. Every intra-document walk travels in this
//!   space, because a walk that never leaves the document has nothing to check a generation
//!   against and the check is not free. [`OptIndex`] is its optional form, in the same four bytes.
//! * [`NodeKey`] adds the occupancy counter and the arena identity, so it can be stored, handed to
//!   another subsystem and resolved later without ever naming a node that has since been replaced.
//! * [`opaque`] is what the name looks like to an engine that wants a bare integer or a bare
//!   pointer.

pub mod document_id;
pub mod node_key;
pub mod opaque;

pub use crate::id::document_id::{DocumentId, NODE_ARENA};
pub use crate::id::node_key::{NodeIndex, NodeKey, OptIndex};
