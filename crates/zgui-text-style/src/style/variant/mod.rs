//! The properties that select among a face's optional behaviours.
//!
//! Six CSS properties are collected here — `font-kerning` and the five `font-variant-*` longhands —
//! because they are one mechanism wearing six names. None of them selects a different face, sets an
//! axis or moves a line; each turns one of the chosen face's optional substitutions on or off, and
//! every one of them resolves into the same thing: entries in the OpenType feature list a shaper is
//! handed. [`resolve`] is where that happens, once, for all six.
//!
//! Every one of them is a *shaping* property. A feature substitutes glyphs and changes advances, so
//! a change to any of these invalidates a shaped result and cannot be answered by breaking the
//! glyphs already there differently.
//!
//! ```
//! use zgui_text_style::{FontVariant, ShapingKey, TextStyle, variant};
//!
//! let plain = TextStyle::initial();
//! let mut unkerned = TextStyle::initial();
//! unkerned.variant.kerning = variant::FontKerning::None;
//!
//! // The property reaches the feature list the shaper is given …
//! assert!(plain.shaping_features().is_empty());
//! assert_eq!(unkerned.shaping_features().len(), 1);
//!
//! // … and the shaping key, so a run already shaped is re-shaped rather than reused.
//! assert_ne!(ShapingKey::of(&plain), ShapingKey::of(&unkerned));
//! assert_eq!(FontVariant::initial(), plain.variant);
//! ```

pub mod caps;
pub mod east_asian;
pub mod kerning;
pub mod ligatures;
pub mod numeric;
pub mod position;
pub mod resolve;

use crate::key::digest::Digest;

pub use crate::style::variant::caps::FontVariantCaps;
pub use crate::style::variant::east_asian::{EastAsianForms, EastAsianWidth, FontVariantEastAsian};
pub use crate::style::variant::kerning::FontKerning;
pub use crate::style::variant::ligatures::{FontVariantLigatures, LigatureSetting};
pub use crate::style::variant::numeric::{
    FontVariantNumeric, NumericFigures, NumericFractions, NumericSpacing,
};
pub use crate::style::variant::position::FontVariantPosition;

/// The six feature-selecting properties of one run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontVariant {
    /// `font-kerning`.
    pub kerning: FontKerning,
    /// `font-variant-ligatures`.
    pub ligatures: FontVariantLigatures,
    /// `font-variant-caps`.
    pub caps: FontVariantCaps,
    /// `font-variant-position`.
    pub position: FontVariantPosition,
    /// `font-variant-numeric`.
    pub numeric: FontVariantNumeric,
    /// `font-variant-east-asian`.
    pub east_asian: FontVariantEastAsian,
}

impl FontVariant {
    /// The value a document with no rules at all resolves to, which asks for nothing.
    pub fn initial() -> Self {
        Self::default()
    }

    /// Whether nothing at all is asked for, so that no feature is owed.
    pub fn is_initial(&self) -> bool {
        *self == Self::default()
    }

    /// The OpenType features these six properties ask for.
    pub fn features(&self) -> resolve::Features {
        let mut features = resolve::Features::new();
        resolve::append(self, &mut features);
        features
    }

    /// Mixes the whole group into a digest.
    pub(crate) fn hash_into(&self, digest: &mut Digest) {
        digest.push(self);
    }
}
