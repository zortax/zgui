//! Text properties, lowered out of the cascade and split into the two halves that cost different
//! amounts of work.
//!
//! # The split, and why everything here is arranged around it
//!
//! Laying out a paragraph is two steps. *Shaping* turns characters into positioned glyphs: it
//! consults the face, applies its features and kerning, and is expensive. *Breaking* decides where
//! the lines fall in a given width: it walks the glyphs the first step produced and is cheap —
//! measured on a thousand words, about a twenty-eighth of a shape.
//!
//! A layout engine asks a paragraph how wide and how tall it is at many candidate widths while it
//! resolves the flex or grid around it. If each of those questions cost a shape, text would
//! dominate the frame. So a style is hashed twice: [`ShapingKey`] over everything that decides
//! which glyphs exist, [`BreakingKey`] over everything that decides only where the lines fall. A
//! width change moves the second and not the first, and the shaped result stays valid.
//!
//! [`TextDamage`] is the same split seen from the restyle side: given the style an element had and
//! the style it now has, it answers whether the text must be shaped again, broken again, or left
//! alone. It is *derived from the keys*, so a property cannot be classified one way and hashed the
//! other.
//!
//! # What is here
//!
//! | Module | Contents |
//! |---|---|
//! | [`style`] | [`TextStyle`], [`ParagraphStyle`], [`TextPaint`] and the value types they hold |
//! | [`key`] | [`ShapingKey`], [`BreakingKey`] and the [`Digest`] both are built with |
//! | [`lower`] | cascaded style → the types above, and the cache that does it once per style |
//! | [`damage`] | [`TextDamage`] |
//!
//! ```
//! use zgui_css::StyleDraft;
//! use zgui_css::values::text::TextAlignKeyword;
//! use zgui_text_style::{BreakingKey, ShapingKey, lower};
//!
//! let before = StyleDraft::initial().build();
//!
//! let mut draft = StyleDraft::from_style(&before);
//! draft.inherited_text().text_align = TextAlignKeyword::Center;
//! let after = draft.build();
//!
//! // Centring a paragraph moves no glyph, so the shaped result survives it.
//! assert_eq!(
//!     ShapingKey::of(&lower::text_style(&before)),
//!     ShapingKey::of(&lower::text_style(&after)),
//! );
//! assert_ne!(
//!     BreakingKey::of_paragraph(&lower::paragraph_style(&before)),
//!     BreakingKey::of_paragraph(&lower::paragraph_style(&after)),
//! );
//! ```
//!
//! # Where the colour is
//!
//! Not in [`TextStyle`], and not in either key. A run's colour is lowered separately, into
//! [`TextPaint`], which carries the colour beside the identity of the cascade result it came from.
//! A consumer claims one brush slot per such identity and stores the *slot* in its shaped result,
//! so switching theme rewrites a handful of table entries instead of re-shaping every string in the
//! application.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod damage;
pub mod key;
pub mod lower;
pub mod parity;
pub mod style;

pub use crate::damage::TextDamage;
pub use crate::key::{BreakingKey, Digest, ShapingKey};
pub use crate::lower::cache::{TextStyleCache, TextStyleKey};
pub use crate::lower::set::TextStyleSet;
pub use crate::style::variant;
pub use crate::style::{
    DEFAULT_OBLIQUE_DEGREES, Direction, FamilyName, FontFamilyList, FontFeature, FontSlant,
    FontVariant, FontVariation, GenericFamily, LengthPercent, LineBreak, LineHeight,
    OPTICAL_SIZE_AXIS, OpticalSizing, OverflowWrap, ParagraphStyle, SynthesisWeight, TextAlign,
    TextAlignLast, TextIndent, TextJustify, TextPaint, TextPaintKey, TextStyle, WhiteSpaceCollapse,
    WordBreak, WrapMode, WritingMode, tag,
};
