//! Faces: where they come from, what they can draw, and what identifies one.

mod bridge;
pub mod color;
mod query;
mod register;
pub(crate) mod registry;
pub(crate) mod resolve;
pub mod script;
mod source;

pub use crate::font::color::ColorSupport;
pub use crate::font::script::script_of;
