//! What a style change costs the text pipeline.
//!
//! ```
//! use zgui_css::StyleDraft;
//! use zgui_css::values::font::{FontSize, FontSizeExt};
//! use zgui_geom::CssPx;
//! use zgui_text_style::TextDamage;
//!
//! let before = StyleDraft::initial().build();
//!
//! let mut draft = StyleDraft::from_style(&before);
//! draft.font().font_size = FontSize::for_px(CssPx(20.0));
//! let after = draft.build();
//!
//! assert_eq!(TextDamage::between(&before, &after), TextDamage::RESHAPE);
//! ```

pub mod classify;

pub use crate::damage::classify::TextDamage;
