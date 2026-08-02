//! Two hashes over one style, and why there are two.
//!
//! Shaping — turning characters into positioned glyphs — is expensive. Breaking those glyphs into
//! lines is cheap: measured on a thousand words, a re-break costs about a twenty-eighth of a fresh
//! shape. A layout engine asks a text leaf for its size at many different widths while it resolves
//! the surrounding flex or grid, so the difference between the two decides whether laying out a
//! paragraph is a background detail or the frame's dominant cost.
//!
//! Splitting the style in half is what lets a consumer tell the two apart mechanically rather than
//! by inspection. [`ShapingKey`] covers everything that changes the glyphs; [`BreakingKey`] covers
//! everything that changes only where the lines fall. A property belongs to exactly one of them.
//!
//! ```
//! use zgui_text_style::{BreakingKey, ShapingKey, TextStyle};
//!
//! let narrow = TextStyle::initial();
//! let mut wider = TextStyle::initial();
//! wider.letter_spacing = zgui_text_style::LengthPercent::length(zgui_geom::CssPx(1.0));
//!
//! // Letter spacing is baked into cluster advances, so it is a shaping change.
//! assert_ne!(ShapingKey::of(&narrow), ShapingKey::of(&wider));
//!
//! let mut broken_differently = TextStyle::initial();
//! broken_differently.overflow_wrap = zgui_text_style::OverflowWrap::BreakWord;
//!
//! // Emergency breaking inside a word changes no glyph, so it is a breaking change.
//! assert_eq!(ShapingKey::of(&narrow), ShapingKey::of(&broken_differently));
//! assert_ne!(BreakingKey::of(&narrow), BreakingKey::of(&broken_differently));
//! ```

pub mod breaking;
pub mod digest;
pub mod shaping;

pub use crate::key::breaking::BreakingKey;
pub use crate::key::digest::Digest;
pub use crate::key::shaping::ShapingKey;
