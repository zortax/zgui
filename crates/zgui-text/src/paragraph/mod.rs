//! Paragraphs: shaping them once, breaking them many times.

pub mod band;
pub mod break_request;
pub mod broken;
pub mod cache;
pub mod content;
pub mod inline_box;
pub mod key;
mod recall;
pub mod shaped;
pub mod shaper;

pub use crate::paragraph::band::{LineBand, LineBands};
pub use crate::paragraph::break_request::BreakRequest;
pub use crate::paragraph::broken::BrokenParagraph;
pub use crate::paragraph::cache::{ParagraphCache, lay_out};
pub use crate::paragraph::content::{ParagraphContent, StyledRun};
pub use crate::paragraph::inline_box::{InlineBoxGeometry, InlineBoxPlacement};
pub use crate::paragraph::key::{ContentKey, MaxAdvance, ParagraphKey, breaking_key};
pub use crate::paragraph::shaped::{ContentWidths, Plan, ShapedParagraph};
pub use crate::paragraph::shaper::ParagraphShaper;
