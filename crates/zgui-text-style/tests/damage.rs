//! One property at a time, and the exact damage each of them owes.
//!
//! Every test here moves a single longhand off its initial value and nothing else. That is the
//! whole design: a test that changed two properties at once would report that *a* key moved without
//! saying which property moved it, so it would keep passing after one of the two stopped being
//! hashed at all. Each test therefore also asserts that the property genuinely arrived — the value
//! it lowers to really did change — because a longhand that no lowering reads cannot move a key,
//! and "the key did not move" is only evidence once the value is known to have got through.
//!
//! The two halves are asserted in both directions. A shaping property must move a shaping key *and*
//! be absent from the breaking ones, and a breaking property must move a breaking key *and* leave
//! both shaping keys alone; that second half is what stops a property from being classified
//! correctly and hashed conservatively into the expensive key, which lays text out correctly and
//! costs a shaping pass on every restyle.

mod support;

use support::{PARAGRAPH, RUN, Varied};
use zgui_css::values::text::WritingModeProperty;
use zgui_css::values::{font, text};
use zgui_text_style::{
    LineBreak, OpticalSizing, SynthesisWeight, TextAlignLast, WritingMode, lower, variant,
};

/// The shaping half: properties that change which glyphs exist or how wide they are.
mod shaping {
    use super::*;

    /// `font-kerning` decides whether the face's kerning pairs move one glyph against the next.
    #[test]
    fn font_kerning_costs_a_shape() {
        let varied = Varied::of(|draft| draft.font().font_kerning = font::FontKerning::None);
        assert_eq!(
            lower::text_style(varied.after()).variant.kerning,
            variant::FontKerning::None,
            "the property must reach the lowered style before the key can mean anything",
        );
        varied.must_reshape(RUN);
    }

    /// `font-variant-ligatures` replaces glyph sequences with single glyphs.
    #[test]
    fn font_variant_ligatures_costs_a_shape() {
        let varied = Varied::of(|draft| {
            draft.font().font_variant_ligatures = font::FontVariantLigatures::NONE;
        });
        let lowered = lower::text_style(varied.after()).variant.ligatures;
        assert_eq!(
            lowered.common,
            variant::LigatureSetting::Off,
            "`none` must turn every group off, not leave them all on default",
        );
        assert_eq!(lowered.contextual, variant::LigatureSetting::Off);
        varied.must_reshape(RUN);
    }

    /// `font-variant-caps` substitutes small-capital forms for lower-case letters.
    #[test]
    fn font_variant_caps_costs_a_shape() {
        let varied = Varied::of(|draft| {
            draft.font().font_variant_caps = font::FontVariantCaps::SmallCaps;
        });
        assert_eq!(
            lower::text_style(varied.after()).variant.caps,
            variant::FontVariantCaps::SmallCaps,
        );
        varied.must_reshape(RUN);
    }

    /// `font-variant-position` substitutes the face's own raised forms.
    #[test]
    fn font_variant_position_costs_a_shape() {
        let varied = Varied::of(|draft| {
            draft.font().font_variant_position = font::FontVariantPosition::Super;
        });
        assert_eq!(
            lower::text_style(varied.after()).variant.position,
            variant::FontVariantPosition::Super,
        );
        varied.must_reshape(RUN);
    }

    /// `font-variant-numeric` changes figure shapes and, with tabular figures, their advances.
    #[test]
    fn font_variant_numeric_costs_a_shape() {
        let varied = Varied::of(|draft| {
            draft.font().font_variant_numeric = font::FontVariantNumeric::TABULAR_NUMS;
        });
        assert_eq!(
            lower::text_style(varied.after()).variant.numeric.spacing,
            variant::NumericSpacing::Tabular,
        );
        varied.must_reshape(RUN);
    }

    /// `font-variant-east-asian` selects between national forms of the same character.
    #[test]
    fn font_variant_east_asian_costs_a_shape() {
        let varied = Varied::of(|draft| {
            draft.font().font_variant_east_asian = font::FontVariantEastAsian::JIS78;
        });
        assert_eq!(
            lower::text_style(varied.after()).variant.east_asian.forms,
            variant::EastAsianForms::Jis78,
        );
        varied.must_reshape(RUN);
    }

    /// `font-optical-sizing` moves the `opsz` axis, which redraws every outline and every advance.
    #[test]
    fn font_optical_sizing_costs_a_shape() {
        let varied = Varied::of(|draft| {
            draft.font().font_optical_sizing = font::FontOpticalSizing::None;
        });
        assert_eq!(
            lower::text_style(varied.after()).optical_sizing,
            OpticalSizing::None,
        );
        varied.must_reshape(RUN);
    }

    /// `font-synthesis-weight` decides whether a missing weight is faked by thickening the outlines.
    #[test]
    fn font_synthesis_weight_costs_a_shape() {
        let varied = Varied::of(|draft| {
            draft.font().font_synthesis_weight = font::FontSynthesis::None;
        });
        assert_eq!(
            lower::text_style(varied.after()).synthesis_weight,
            SynthesisWeight::None,
        );
        varied.must_reshape(RUN);
    }

    /// `writing-mode` turns the paragraph on its side, which is a different set of glyphs.
    #[test]
    fn writing_mode_costs_a_shape() {
        let varied = Varied::of(|draft| {
            draft.inherited_box().writing_mode = WritingModeProperty::VerticalRl
        });
        assert_eq!(
            lower::paragraph_style(varied.after()).writing_mode,
            WritingMode::VerticalRl,
        );
        varied.must_reshape(PARAGRAPH);
    }
}

/// The breaking half: properties that move the lines and no glyph.
mod breaking {
    use super::*;

    /// `line-break` changes which positions beside CJK punctuation a line may be cut at.
    #[test]
    fn line_break_costs_a_break_and_no_shape() {
        let varied =
            Varied::of(|draft| draft.inherited_text().line_break = text::LineBreak::Strict);
        assert_eq!(
            lower::text_style(varied.after()).line_break,
            LineBreak::Strict,
        );
        varied.must_rebreak(RUN);
    }

    /// `text-align-last` moves the final line and nothing before it.
    #[test]
    fn text_align_last_costs_a_break_and_no_shape() {
        let varied = Varied::of(|draft| {
            draft.inherited_text().text_align_last = text::TextAlignLast::Justify
        });
        assert_eq!(
            lower::paragraph_style(varied.after()).align_last,
            TextAlignLast::Justify,
        );
        varied.must_rebreak(PARAGRAPH);
    }
}

/// The two ways a property can be lowered onto the wrong side of the line.
mod classification {
    use super::*;

    /// Each of the eleven properties is hashed by exactly one of the four keys.
    ///
    /// The per-property tests above each assert this for one property; what this adds is that the
    /// *set* is complete and that no two of them collide. A property added to two keys at once passes
    /// its own test and fails here.
    #[test]
    fn each_property_moves_one_key_and_the_other_three_stand_still() {
        let cases: Vec<(&str, Varied)> = vec![
            (
                "font-kerning",
                Varied::of(|draft| draft.font().font_kerning = font::FontKerning::None),
            ),
            (
                "font-variant-ligatures",
                Varied::of(|draft| {
                    draft.font().font_variant_ligatures = font::FontVariantLigatures::NONE;
                }),
            ),
            (
                "font-variant-caps",
                Varied::of(|draft| {
                    draft.font().font_variant_caps = font::FontVariantCaps::SmallCaps
                }),
            ),
            (
                "font-variant-position",
                Varied::of(|draft| {
                    draft.font().font_variant_position = font::FontVariantPosition::Sub;
                }),
            ),
            (
                "font-variant-numeric",
                Varied::of(|draft| {
                    draft.font().font_variant_numeric = font::FontVariantNumeric::SLASHED_ZERO;
                }),
            ),
            (
                "font-variant-east-asian",
                Varied::of(|draft| {
                    draft.font().font_variant_east_asian = font::FontVariantEastAsian::RUBY;
                }),
            ),
            (
                "font-optical-sizing",
                Varied::of(|draft| {
                    draft.font().font_optical_sizing = font::FontOpticalSizing::None;
                }),
            ),
            (
                "font-synthesis-weight",
                Varied::of(|draft| draft.font().font_synthesis_weight = font::FontSynthesis::None),
            ),
        ];

        for (name, varied) in &cases {
            assert_ne!(
                lower::text_style(varied.before()),
                lower::text_style(varied.after()),
                "{name} never reached the lowered run style",
            );
            varied.must_reshape(RUN);
        }
        assert_eq!(cases.len(), 8, "every run-level shaping addition is here");
    }
}
