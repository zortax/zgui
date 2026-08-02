//! Text nodes: what they hold, and whose style they are drawn with.

pub mod inherit;
pub mod node;

pub use crate::text::inherit::inherits_from;
pub use crate::text::node::{set_text, text_of};
