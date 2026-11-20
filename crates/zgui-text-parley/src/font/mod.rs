//! Faces: where they come from and what identifies one.

mod bridge;
pub(crate) mod query;
mod register;
pub(crate) mod registry;
pub(crate) mod resolve;
pub mod script;
mod source;

pub use crate::font::script::script_of;
