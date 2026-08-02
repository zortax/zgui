//! Changing a document that something has already looked at.
//!
//! Building a document and editing one are different operations with different contracts. Building
//! writes a node's name, classes and state directly, because nothing downstream has seen it yet.
//! Editing has to record what the element looked like before the change, tell its ancestors that
//! there is work below them, and decide whether the change can affect any style at all.
//!
//! The last of those three is [`filter`], and it is here rather than beside the style engine
//! because its consumer is here: the decision "this change cannot affect any computed style" is
//! taken at the moment of the change, before anything is recorded, and the answer it needs is
//! about the element being changed.
//!
//! | Module | Contents |
//! |---|---|
//! | [`edit`] | the batched change API, and the state a document needs to accept one |
//! | [`snapshot`] | what an element looked like before it changed |
//! | [`ancestors`] | telling a node's ancestors that there is work below them |
//! | [`structure`] | which siblings a change to a child list can have changed the match of |
//! | [`ordinals`] | comparing children by position, once, after every change that moved one |
//! | [`hints`] | how much of the style engine's work each change needs redone |
//! | [`filter`] | deciding that a change cannot affect any computed style |

pub mod ancestors;
pub mod edit;
pub mod filter;
pub mod hints;
pub mod ordinals;
pub mod snapshot;
pub mod structure;

pub use crate::mutate::edit::Edit;
pub use crate::mutate::edit::guard::Poisoned;
pub use crate::mutate::filter::{EverythingMatters, StyleFilter};
pub use crate::mutate::hints::HintLog;
pub use crate::mutate::snapshot::SnapshotStore;
pub use crate::mutate::structure::StructureLog;
