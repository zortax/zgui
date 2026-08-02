//! The cascaded result: what a style is, and how to make one without a cascade.

pub mod draft;
pub mod inherit;
pub mod pinned;
pub mod style;

pub use crate::computed::draft::StyleDraft;
pub use crate::computed::inherit::inherited_style;
pub use crate::computed::pinned::PinnedGroup;
pub use crate::computed::style::{ComputedStyle, StructPtr, style_structs};
