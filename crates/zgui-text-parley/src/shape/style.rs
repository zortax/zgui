//! Lowering one run's style onto the engine's style properties.

use core::ops::Range;

use parley::style::{FontFamily, FontFamilyName, FontVariations, LineHeight, StyleProperty};
use parley::{FontFeatures, OverflowWrap, RangedBuilder, TextWrapMode, WordBreak};
use zgui_geom::CssPx;
use zgui_text::Brush;
use zgui_text_style::{
    FamilyName, FontFamilyList, LineHeight as CssLineHeight, OverflowWrap as CssOverflowWrap,
    TextStyle, WordBreak as CssWordBreak, WrapMode,
};

use crate::font::resolve::generic_family;
use crate::shape::brush::SlotBrush;

/// The style properties one run contributes, in the engine's vocabulary.
///
/// Built as an owned list rather than pushed directly because the family list has to outlive the
/// call that pushes it, and because a paragraph's default style and a run's style are pushed
/// through two different methods that must not drift apart.
pub(crate) struct LoweredStyle {
    /// The properties, in a fixed order.
    properties: Vec<StyleProperty<'static, SlotBrush>>,
}

impl LoweredStyle {
    /// Lowers one style and its brush.
    ///
    /// `space_advance` resolves the percentage half of `word-spacing`, which is measured against
    /// the advance of a space in the face that was chosen and so cannot be resolved from a style
    /// alone. Zero is the right value to pass when no face has been chosen yet, which leaves a
    /// percentage word-spacing contributing nothing rather than contributing a guess.
    pub(crate) fn of(style: &TextStyle, brush: Brush, space_advance: CssPx) -> Self {
        let properties = vec![
            StyleProperty::FontFamily(family(&style.family)),
            StyleProperty::FontSize(style.size.0),
            StyleProperty::FontWeight(parley::FontWeight::new(style.weight)),
            StyleProperty::FontStyle(crate::font::resolve::slant(style.slant)),
            StyleProperty::FontWidth(parley::FontWidth::from_ratio(style.width)),
            StyleProperty::FontVariations(variations(style)),
            StyleProperty::FontFeatures(features(style)),
            StyleProperty::LineHeight(line_height(style.line_height)),
            StyleProperty::LetterSpacing(style.letter_spacing.resolve(style.size).0),
            StyleProperty::WordSpacing(style.word_spacing.resolve(space_advance).0),
            StyleProperty::WordBreak(match style.word_break {
                CssWordBreak::Normal => WordBreak::Normal,
                CssWordBreak::BreakAll => WordBreak::BreakAll,
                CssWordBreak::KeepAll => WordBreak::KeepAll,
            }),
            StyleProperty::OverflowWrap(match style.overflow_wrap {
                CssOverflowWrap::Normal => OverflowWrap::Normal,
                CssOverflowWrap::BreakWord => OverflowWrap::BreakWord,
                CssOverflowWrap::Anywhere => OverflowWrap::Anywhere,
            }),
            StyleProperty::TextWrapMode(match style.wrap_mode {
                WrapMode::Wrap => TextWrapMode::Wrap,
                WrapMode::NoWrap => TextWrapMode::NoWrap,
            }),
            StyleProperty::Brush(SlotBrush(brush)),
        ];
        Self { properties }
    }

    /// Pushes the properties as the paragraph's defaults.
    pub(crate) fn push_default(&self, builder: &mut RangedBuilder<'_, SlotBrush>) {
        for property in &self.properties {
            builder.push_default(property.clone());
        }
    }

    /// Pushes the properties over one byte range of the generated string.
    pub(crate) fn push_over(
        &self,
        builder: &mut RangedBuilder<'_, SlotBrush>,
        range: Range<usize>,
    ) {
        for property in &self.properties {
            builder.push(property.clone(), range.clone());
        }
    }
}

/// The engine's spelling of a family list, owned.
fn family(list: &FontFamilyList) -> FontFamily<'static> {
    let names: Vec<FontFamilyName<'static>> = list
        .entries()
        .iter()
        .map(|entry| match entry {
            FamilyName::Named(name) => FontFamilyName::Named(name.as_str().into()),
            FamilyName::Generic(generic) => FontFamilyName::Generic(generic_family(*generic)),
        })
        .collect();
    FontFamily::List(names.into())
}

/// The variable axes this run is instanced at.
///
/// Read through [`TextStyle::shaping_variations`] rather than off the field, because
/// `font-optical-sizing` is a second property that lands on the same list and reading the field
/// would drop it silently.
fn variations(style: &TextStyle) -> FontVariations<'static> {
    let list: Vec<parley::setting::FontVariation> = style
        .shaping_variations()
        .iter()
        .map(|variation| parley::setting::FontVariation {
            tag: parley::setting::Tag::from_bytes(variation.tag.to_be_bytes()),
            value: variation.value,
        })
        .collect();
    FontVariations::List(list.into())
}

/// The OpenType features this run is shaped with.
///
/// Read through [`TextStyle::shaping_features`] rather than off the field, because `font-kerning`
/// and the five `font-variant-*` longhands all resolve into entries of this one list, and reading
/// the field would lower six properties and hand over none of them.
fn features(style: &TextStyle) -> FontFeatures<'static> {
    let list: Vec<parley::setting::FontFeature> = style
        .shaping_features()
        .iter()
        .map(|feature| parley::setting::FontFeature {
            tag: parley::setting::Tag::from_bytes(feature.tag.to_be_bytes()),
            value: u16::try_from(feature.value).unwrap_or(u16::MAX),
        })
        .collect();
    FontFeatures::List(list.into())
}

/// The engine's spelling of `line-height`.
///
/// `normal` is the face's own preferred spacing, which is exactly what a multiple of the face's
/// own metrics means; the other two forms are of the font size and of nothing respectively.
fn line_height(height: CssLineHeight) -> LineHeight {
    match height {
        CssLineHeight::Normal => LineHeight::MetricsRelative(1.0),
        CssLineHeight::Number(multiple) => LineHeight::FontSizeRelative(multiple),
        CssLineHeight::Length(length) => LineHeight::Absolute(length.0),
    }
}

#[cfg(test)]
mod tests {
    use parley::style::StyleProperty;
    use zgui_scene::PaintSlot;
    use zgui_text_style::{OpticalSizing, TextStyle, tag, variant};

    use super::LoweredStyle;

    /// The feature list this run is lowered onto.
    fn features_of(style: &TextStyle) -> Vec<(u32, u16)> {
        let lowered = LoweredStyle::of(style, PaintSlot(0), zgui_geom::CssPx(0.0));
        lowered
            .properties
            .iter()
            .find_map(|property| match property {
                StyleProperty::FontFeatures(parley::FontFeatures::List(list)) => Some(
                    list.iter()
                        .map(|feature| (u32::from_be_bytes(feature.tag.to_bytes()), feature.value))
                        .collect(),
                ),
                _ => None,
            })
            .expect("a lowered style always carries a feature list")
    }

    /// The axis list this run is lowered onto.
    fn variations_of(style: &TextStyle) -> Vec<(u32, f32)> {
        let lowered = LoweredStyle::of(style, PaintSlot(0), zgui_geom::CssPx(0.0));
        lowered
            .properties
            .iter()
            .find_map(|property| match property {
                StyleProperty::FontVariations(parley::style::FontVariations::List(list)) => Some(
                    list.iter()
                        .map(|axis| (u32::from_be_bytes(axis.tag.to_bytes()), axis.value))
                        .collect(),
                ),
                _ => None,
            })
            .expect("a lowered style always carries an axis list")
    }

    /// One property varied, and the exact feature list it must produce.
    type Case = (
        &'static str,
        fn(&mut TextStyle),
        &'static [(&'static [u8; 4], u16)],
    );

    /// Each of the six feature-selecting properties reaches the engine, and reaches it alone.
    ///
    /// This is the half of the fix that the key tests cannot see. A property can be lowered out of
    /// the cascade and hashed into the shaping key and still never be handed to a shaper, in which
    /// case a change to it costs a re-shape that produces the same glyphs — correct output, wasted
    /// work, and a parity claim that is not true. Naming the exact tag and value each property
    /// produces is what makes the pass-through checkable rather than assumed.
    #[test]
    fn every_feature_selecting_property_reaches_the_engine() {
        assert!(
            features_of(&TextStyle::initial()).is_empty(),
            "asking for nothing must send nothing, so the face keeps its own defaults",
        );

        let cases: [Case; 6] = [
            (
                "font-kerning: none",
                |style| style.variant.kerning = variant::FontKerning::None,
                &[(b"kern", 0)],
            ),
            (
                "font-variant-ligatures: none",
                |style| style.variant.ligatures = variant::FontVariantLigatures::none(),
                &[
                    (b"liga", 0),
                    (b"clig", 0),
                    (b"dlig", 0),
                    (b"hlig", 0),
                    (b"calt", 0),
                ],
            ),
            (
                "font-variant-caps: small-caps",
                |style| style.variant.caps = variant::FontVariantCaps::SmallCaps,
                &[(b"smcp", 1)],
            ),
            (
                "font-variant-position: super",
                |style| style.variant.position = variant::FontVariantPosition::Super,
                &[(b"sups", 1)],
            ),
            (
                "font-variant-numeric: tabular-nums slashed-zero",
                |style| {
                    style.variant.numeric.spacing = variant::NumericSpacing::Tabular;
                    style.variant.numeric.slashed_zero = true;
                },
                &[(b"tnum", 1), (b"zero", 1)],
            ),
            (
                "font-variant-east-asian: jis04 ruby",
                |style| {
                    style.variant.east_asian.forms = variant::EastAsianForms::Jis04;
                    style.variant.east_asian.ruby = true;
                },
                &[(b"jp04", 1), (b"ruby", 1)],
            ),
        ];

        for (name, apply, expected) in cases {
            let mut style = TextStyle::initial();
            apply(&mut style);
            let wanted: Vec<(u32, u16)> = expected
                .iter()
                .map(|(name, value)| (tag(name), *value))
                .collect();
            assert_eq!(features_of(&style), wanted, "{name}");
        }
    }

    /// `font-optical-sizing` reaches the engine as the axis it actually is.
    #[test]
    fn optical_sizing_reaches_the_engine_as_the_opsz_axis() {
        let style = TextStyle::initial();
        assert_eq!(
            variations_of(&style),
            vec![(tag(b"opsz"), 16.0)],
            "`auto` drives the axis from the font size",
        );

        let mut fixed = TextStyle::initial();
        fixed.optical_sizing = OpticalSizing::None;
        assert!(
            variations_of(&fixed).is_empty(),
            "`none` leaves the axis wherever the face puts it",
        );
    }
}
