//! The six feature-selecting properties, out of the cascade.
//!
//! Three of them arrive as flag sets rather than as keywords, because their grammars let an author
//! name several independent things at once. The parser has already rejected the combinations the
//! grammar forbids — `lining-nums oldstyle-nums`, `jis78 jis04`, `common-ligatures
//! no-common-ligatures` — so each group of mutually exclusive flags can be read as one choice here,
//! and is: an enumeration with one variant per flag makes the impossible states unrepresentable
//! from this point on rather than merely unreachable.

use zgui_css::computed::style_structs;
use zgui_css::values::font;

use crate::style::variant::FontVariant;
use crate::style::variant::caps::FontVariantCaps;
use crate::style::variant::east_asian::{EastAsianForms, EastAsianWidth, FontVariantEastAsian};
use crate::style::variant::kerning::FontKerning;
use crate::style::variant::ligatures::{FontVariantLigatures, LigatureSetting};
use crate::style::variant::numeric::{
    FontVariantNumeric, NumericFigures, NumericFractions, NumericSpacing,
};
use crate::style::variant::position::FontVariantPosition;

/// All six at once.
pub fn variant(group: &style_structs::Font) -> FontVariant {
    FontVariant {
        kerning: kerning(group),
        ligatures: ligatures(group),
        caps: caps(group),
        position: position(group),
        numeric: numeric(group),
        east_asian: east_asian(group),
    }
}

/// `font-kerning`.
pub fn kerning(group: &style_structs::Font) -> FontKerning {
    match group.font_kerning {
        font::FontKerning::Auto => FontKerning::Auto,
        font::FontKerning::Normal => FontKerning::Normal,
        font::FontKerning::None => FontKerning::None,
    }
}

/// `font-variant-caps`.
pub fn caps(group: &style_structs::Font) -> FontVariantCaps {
    match group.font_variant_caps {
        font::FontVariantCaps::Normal => FontVariantCaps::Normal,
        font::FontVariantCaps::SmallCaps => FontVariantCaps::SmallCaps,
    }
}

/// `font-variant-position`.
pub fn position(group: &style_structs::Font) -> FontVariantPosition {
    match group.font_variant_position {
        font::FontVariantPosition::Normal => FontVariantPosition::Normal,
        font::FontVariantPosition::Sub => FontVariantPosition::Sub,
        font::FontVariantPosition::Super => FontVariantPosition::Super,
    }
}

/// `font-variant-ligatures`.
///
/// `none` is a flag of its own rather than the four disabling flags together, so it is checked
/// first; without that, `font-variant-ligatures: none` would lower to four `Auto` settings and turn
/// nothing off at all.
pub fn ligatures(group: &style_structs::Font) -> FontVariantLigatures {
    let flags = group.font_variant_ligatures;
    if flags.contains(font::FontVariantLigatures::NONE) {
        return FontVariantLigatures::none();
    }
    FontVariantLigatures {
        common: setting(
            flags,
            font::FontVariantLigatures::COMMON_LIGATURES,
            font::FontVariantLigatures::NO_COMMON_LIGATURES,
        ),
        discretionary: setting(
            flags,
            font::FontVariantLigatures::DISCRETIONARY_LIGATURES,
            font::FontVariantLigatures::NO_DISCRETIONARY_LIGATURES,
        ),
        historical: setting(
            flags,
            font::FontVariantLigatures::HISTORICAL_LIGATURES,
            font::FontVariantLigatures::NO_HISTORICAL_LIGATURES,
        ),
        contextual: setting(
            flags,
            font::FontVariantLigatures::CONTEXTUAL,
            font::FontVariantLigatures::NO_CONTEXTUAL,
        ),
    }
}

/// One ligature group's setting, from the pair of flags that turn it on and off.
fn setting(
    flags: font::FontVariantLigatures,
    on: font::FontVariantLigatures,
    off: font::FontVariantLigatures,
) -> LigatureSetting {
    if flags.contains(on) {
        LigatureSetting::On
    } else if flags.contains(off) {
        LigatureSetting::Off
    } else {
        LigatureSetting::Auto
    }
}

/// `font-variant-numeric`.
pub fn numeric(group: &style_structs::Font) -> FontVariantNumeric {
    let flags = group.font_variant_numeric;
    FontVariantNumeric {
        figures: if flags.contains(font::FontVariantNumeric::LINING_NUMS) {
            NumericFigures::Lining
        } else if flags.contains(font::FontVariantNumeric::OLDSTYLE_NUMS) {
            NumericFigures::Oldstyle
        } else {
            NumericFigures::Auto
        },
        spacing: if flags.contains(font::FontVariantNumeric::PROPORTIONAL_NUMS) {
            NumericSpacing::Proportional
        } else if flags.contains(font::FontVariantNumeric::TABULAR_NUMS) {
            NumericSpacing::Tabular
        } else {
            NumericSpacing::Auto
        },
        fractions: if flags.contains(font::FontVariantNumeric::DIAGONAL_FRACTIONS) {
            NumericFractions::Diagonal
        } else if flags.contains(font::FontVariantNumeric::STACKED_FRACTIONS) {
            NumericFractions::Stacked
        } else {
            NumericFractions::Auto
        },
        ordinal: flags.contains(font::FontVariantNumeric::ORDINAL),
        slashed_zero: flags.contains(font::FontVariantNumeric::SLASHED_ZERO),
    }
}

/// `font-variant-east-asian`.
pub fn east_asian(group: &style_structs::Font) -> FontVariantEastAsian {
    let flags = group.font_variant_east_asian;
    let forms = [
        (font::FontVariantEastAsian::JIS78, EastAsianForms::Jis78),
        (font::FontVariantEastAsian::JIS83, EastAsianForms::Jis83),
        (font::FontVariantEastAsian::JIS90, EastAsianForms::Jis90),
        (font::FontVariantEastAsian::JIS04, EastAsianForms::Jis04),
        (
            font::FontVariantEastAsian::SIMPLIFIED,
            EastAsianForms::Simplified,
        ),
        (
            font::FontVariantEastAsian::TRADITIONAL,
            EastAsianForms::Traditional,
        ),
    ]
    .into_iter()
    .find_map(|(flag, form)| flags.contains(flag).then_some(form))
    .unwrap_or(EastAsianForms::Auto);

    FontVariantEastAsian {
        forms,
        width: if flags.contains(font::FontVariantEastAsian::FULL_WIDTH) {
            EastAsianWidth::FullWidth
        } else if flags.contains(font::FontVariantEastAsian::PROPORTIONAL_WIDTH) {
            EastAsianWidth::ProportionalWidth
        } else {
            EastAsianWidth::Auto
        },
        ruby: flags.contains(font::FontVariantEastAsian::RUBY),
    }
}
