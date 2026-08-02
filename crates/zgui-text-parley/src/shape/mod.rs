//! Shaping paragraphs once and breaking them many times.

pub(crate) mod bands;
pub(crate) mod boxes;
pub(crate) mod breaking;
pub mod brush;
pub(crate) mod build;
pub(crate) mod clusters;
pub mod engine;
pub(crate) mod glyphs;
pub(crate) mod lines;
pub mod shaper;
pub(crate) mod strut;
pub(crate) mod style;

pub use crate::shape::brush::SlotBrush;
pub use crate::shape::engine::ShapedLayout;
pub use crate::shape::shaper::Shaper;
