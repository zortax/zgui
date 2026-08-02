//! Face selection and sizing.

use zgui_geom::CssPx;

/// `font-kerning`, which says whether the face's own kerning is applied.
pub use style::computed_values::font_kerning::T as FontKerning;
/// `font-optical-sizing`, which says whether a variable face's `opsz` axis follows the font size.
pub use style::computed_values::font_optical_sizing::T as FontOpticalSizing;
/// `font-variant-caps`, which selects small-capital forms.
///
/// This build offers `normal` and `small-caps` only: the remaining five keywords of the CSS
/// grammar are generated for another engine and are not values the parser here accepts.
pub use style::computed_values::font_variant_caps::T as FontVariantCaps;
/// `font-variant-position`, which selects the face's own superscript and subscript forms.
pub use style::computed_values::font_variant_position::T as FontVariantPosition;
/// The `font-family` list, in the order the author wrote it.
pub use style::values::computed::FontFamily;
/// One `font-feature-settings` block: OpenType tags and their values.
pub use style::values::computed::FontFeatureSettings;
/// `font-language-override`, which forces a face's language system.
pub use style::values::computed::FontLanguageOverride;
/// `font-size`, carrying both the computed size and the size actually used.
pub use style::values::computed::FontSize;
/// `font-width` (historically `font-stretch`), as a percentage of normal.
pub use style::values::computed::FontStretch;
/// `font-style`, which is normal, italic or an oblique angle.
pub use style::values::computed::FontStyle;
/// `font-synthesis-weight`, which says whether a bolder face may be faked when none is installed.
pub use style::values::computed::FontSynthesis;
/// The `font-variant-east-asian` flags, which select between national and legacy CJK forms.
pub use style::values::computed::FontVariantEastAsian;
/// The `font-variant-ligatures` flags, each of which turns one ligature group on or off.
pub use style::values::computed::FontVariantLigatures;
/// The `font-variant-numeric` flags, which pick figure shapes, spacing and fraction forms.
pub use style::values::computed::FontVariantNumeric;
/// One `font-variation-settings` block: variable-axis tags and their values.
pub use style::values::computed::FontVariationSettings;
/// `font-weight`, as a number between 1 and 1000.
pub use style::values::computed::FontWeight;
/// `line-height`, which is `normal`, a multiple of the font size, or a length.
pub use style::values::computed::LineHeight;
/// The document language an element's text is in, which the `lang` attribute sets.
pub use style::values::computed::XLang;
/// The generic families the cascade can name, each of which resolves to a configured default face.
pub use style::values::computed::font::GenericFontFamily;
/// One entry of a family list: either a name or a generic.
pub use style::values::computed::font::SingleFontFamily;

/// Constructs a `font-size` from a length, as a cascade with no keyword involved would.
///
/// ```
/// use zgui_css::values::font::{FontSize, FontSizeExt, size_in_css_px};
/// use zgui_geom::CssPx;
///
/// assert_eq!(size_in_css_px(&FontSize::for_px(CssPx(18.5))), CssPx(18.5));
/// ```
pub trait FontSizeExt {
    /// A `font-size` of exactly this many CSS pixels.
    fn for_px(size: CssPx) -> Self;
}

impl FontSizeExt for FontSize {
    fn for_px(size: CssPx) -> Self {
        let length = style::values::computed::NonNegativeLength::new(size.0);
        Self {
            computed_size: length,
            used_size: length,
            keyword_info: style::values::specified::font::KeywordInfo::none(),
        }
    }
}

/// The used font size, in this framework's own unit.
///
/// This is the size glyphs are actually drawn at, which is the computed size after any minimum the
/// platform imposes — so it is the one a shaper must be handed.
pub fn size_in_css_px(size: &FontSize) -> CssPx {
    CssPx(size.used_size().px())
}

/// The numeric weight, between 1 and 1000.
pub fn weight_value(weight: FontWeight) -> f32 {
    weight.value()
}

/// The width as a fraction of normal, where `1.0` is `normal` and `0.5` is `ultra-condensed`.
pub fn width_fraction(width: FontStretch) -> f32 {
    width.to_percentage().0
}

/// Builds a `line-height` from a unitless multiple of the font size.
pub fn line_height_number(multiple: f32) -> LineHeight {
    LineHeight::Number(style::values::generics::NonNegative(multiple))
}

/// Builds a `line-height` from a length.
pub fn line_height_length(length: CssPx) -> LineHeight {
    LineHeight::Length(style::values::computed::NonNegativeLength::new(length.0))
}
