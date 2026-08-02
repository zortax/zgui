//! The font group: family, size, weight, slant, width, variations and features.

use smallvec::SmallVec;
use zgui_css::computed::style_structs;
use zgui_css::values::font;
use zgui_geom::CssPx;
use zgui_interned::Ident;

use crate::style::face::{FontFeature, FontSlant, FontVariation};
use crate::style::family::{FamilyName, FontFamilyList, GenericFamily};
use crate::style::line_height::LineHeight;
use crate::style::optical::OpticalSizing;
use crate::style::synthesis::SynthesisWeight;

/// The family list, dropping the internal placeholder a cascade uses for "no generic".
pub fn family(group: &style_structs::Font) -> FontFamilyList {
    group
        .font_family
        .families
        .iter()
        .filter_map(|entry| match entry {
            font::SingleFontFamily::FamilyName(name) => {
                Some(FamilyName::Named(Ident::new(&name.name)))
            }
            font::SingleFontFamily::Generic(generic) => {
                generic_family(*generic).map(FamilyName::Generic)
            }
        })
        .collect()
}

/// The role a generic family name stands for.
///
/// Returns nothing for the placeholder the cascade uses internally to mean "no generic was named",
/// which is not a family a document can ask for and never appears in an authored list.
pub fn generic_family(generic: font::GenericFontFamily) -> Option<GenericFamily> {
    Some(match generic {
        font::GenericFontFamily::None => return None,
        font::GenericFontFamily::Serif => GenericFamily::Serif,
        font::GenericFontFamily::SansSerif => GenericFamily::SansSerif,
        font::GenericFontFamily::Monospace => GenericFamily::Monospace,
        font::GenericFontFamily::Cursive => GenericFamily::Cursive,
        font::GenericFontFamily::Fantasy => GenericFamily::Fantasy,
        font::GenericFontFamily::SystemUi => GenericFamily::SystemUi,
    })
}

/// The size glyphs are drawn at, which is the used size rather than the computed one.
pub fn size(group: &style_structs::Font) -> CssPx {
    font::size_in_css_px(&group.font_size)
}

/// The numeric weight, between 1 and 1000.
pub fn weight(group: &style_structs::Font) -> f32 {
    font::weight_value(group.font_weight)
}

/// The width as a fraction of normal, where `1.0` is `normal` and `0.5` is `ultra-condensed`.
pub fn width(group: &style_structs::Font) -> f32 {
    font::width_fraction(group.font_stretch)
}

/// The slant, as an angle rather than as a keyword.
pub fn slant(group: &style_structs::Font) -> FontSlant {
    let style = group.font_style;
    if style == font::FontStyle::NORMAL {
        FontSlant::Upright
    } else if style == font::FontStyle::ITALIC {
        FontSlant::Italic
    } else {
        FontSlant::Oblique(style.oblique_degrees())
    }
}

/// `font-synthesis-weight`.
pub fn synthesis_weight(group: &style_structs::Font) -> SynthesisWeight {
    match group.font_synthesis_weight {
        font::FontSynthesis::Auto => SynthesisWeight::Auto,
        font::FontSynthesis::None => SynthesisWeight::None,
    }
}

/// `font-optical-sizing`.
pub fn optical_sizing(group: &style_structs::Font) -> OpticalSizing {
    match group.font_optical_sizing {
        font::FontOpticalSizing::Auto => OpticalSizing::Auto,
        font::FontOpticalSizing::None => OpticalSizing::None,
    }
}

/// `font-variation-settings`, in author order.
pub fn variations(group: &style_structs::Font) -> SmallVec<[FontVariation; 2]> {
    group
        .font_variation_settings
        .0
        .iter()
        .map(|setting| FontVariation {
            tag: setting.tag.0,
            value: setting.value,
        })
        .collect()
}

/// `font-feature-settings`, in author order.
pub fn features(group: &style_structs::Font) -> SmallVec<[FontFeature; 2]> {
    group
        .font_feature_settings
        .0
        .iter()
        .map(|setting| FontFeature {
            tag: setting.tag.0,
            value: setting.value.max(0) as u32,
        })
        .collect()
}

/// The document language, absent when none was declared.
pub fn language(group: &style_structs::Font) -> Option<Ident> {
    let language = &group._x_lang.0;
    (!language.is_empty()).then(|| Ident::new(language))
}

/// The forced OpenType language system, absent when the document forced none.
pub fn language_system(group: &style_structs::Font) -> Option<u32> {
    let tag = group.font_language_override.0;
    (tag != 0).then_some(tag)
}

/// `line-height`, kept in the form it was authored in.
pub fn line_height(group: &style_structs::Font) -> LineHeight {
    match group.line_height {
        font::LineHeight::Normal => LineHeight::Normal,
        font::LineHeight::Number(multiple) => LineHeight::Number(multiple.0),
        font::LineHeight::Length(length) => LineHeight::Length(CssPx(length.px())),
    }
}
