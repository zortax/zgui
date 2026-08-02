//! Turning a cascaded style into this framework's text properties.
//!
//! The cascade produces one record per element holding every CSS property. Shaping text needs
//! about twenty of them, breaking it into lines needs six, and drawing it needs one. Lowering is
//! the step that picks those out and puts them into shapes a shaper and a line breaker can be
//! handed — and, just as importantly, keeps the colour out of the first two, because a run's paint
//! must never be able to invalidate its shaping.
//!
//! ```
//! use zgui_css::StyleDraft;
//! use zgui_text_style::{ShapingKey, lower};
//!
//! let style = StyleDraft::initial().build();
//! let set = lower::style_set(&style);
//!
//! assert_eq!(set.text.size, zgui_geom::CssPx(16.0));
//! assert_eq!(ShapingKey::of(&set.text), ShapingKey::of(&lower::text_style(&style)));
//! ```

pub mod cache;
pub mod font;
pub mod inherited_box;
pub mod inherited_text;
pub mod paint;
pub mod set;
pub mod variant;

pub use crate::lower::cache::TextStyleCache;
pub use crate::lower::set::{paragraph_style, style_set, text_style};
