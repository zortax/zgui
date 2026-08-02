//! Every text property actually arrives, and arrives as the right thing.
//!
//! Without this, the key tests would be satisfied by a lowering that dropped half the cascade: a
//! property that never reaches [`TextStyle`] cannot move a key, so "the key did not move" is only
//! evidence when the value is known to have got through.

use std::sync::Arc;

use zgui_css::values::font::{
    FontSize, FontSizeExt, FontVariantEastAsian as CascadedEastAsian,
    FontVariantLigatures as CascadedLigatures, FontVariantNumeric as CascadedNumeric,
    line_height_length, line_height_number,
};
use zgui_css::values::text::{
    Direction as CascadedDirection, LineBreak as CascadedLineBreak,
    OverflowWrap as CascadedOverflowWrap, TextAlignKeyword, TextAlignLast as CascadedAlignLast,
    TextJustify as CascadedTextJustify, TextWrapMode, WhiteSpaceCollapse as CascadedWhiteSpace,
    WordBreak as CascadedWordBreak, WritingModeProperty as CascadedWritingMode, letter_spacing,
    word_spacing,
};
use zgui_css::{StyleDraft, values};
use zgui_geom::CssPx;
use zgui_interned::Ident;
use zgui_text_style::{
    Direction, FamilyName, GenericFamily, LengthPercent, LineBreak, LineHeight, OverflowWrap,
    ParagraphStyle, TextAlign, TextAlignLast, TextJustify, TextStyle, TextStyleCache,
    WhiteSpaceCollapse, WordBreak, WrapMode, WritingMode, lower, variant,
};

/// The initial cascade lowers to the initial text style.
#[test]
fn the_initial_style_lowers_to_the_initial_text_style() {
    let lowered = lower::text_style(&StyleDraft::initial().build());
    let expected = TextStyle::initial();

    assert_eq!(lowered.size, expected.size);
    assert_eq!(lowered.weight, expected.weight);
    assert_eq!(lowered.slant, expected.slant);
    assert_eq!(lowered.line_height, expected.line_height);
    assert_eq!(lowered.word_break, expected.word_break);
    assert_eq!(lowered.white_space, expected.white_space);
    assert_eq!(lowered.overflow_wrap, expected.overflow_wrap);
    assert_eq!(lowered.wrap_mode, expected.wrap_mode);
    assert_eq!(
        lower::paragraph_style(&StyleDraft::initial().build()),
        ParagraphStyle::initial()
    );
}

/// The family list keeps its author order and its generics.
#[test]
fn the_family_list_keeps_its_order() {
    let mut draft = StyleDraft::initial();
    draft.font().font_family = values::font::FontFamily::serif();
    let lowered = lower::text_style(&draft.build());

    assert_eq!(
        lowered.family.entries(),
        [FamilyName::Generic(GenericFamily::Serif)],
    );
    assert_eq!(lowered.family.first_generic(), Some(GenericFamily::Serif));
}

/// Sizes, weights and slants arrive as numbers rather than keywords.
#[test]
fn the_face_axes_arrive_as_numbers() {
    let mut draft = StyleDraft::initial();
    draft.font().font_size = FontSize::for_px(CssPx(19.0));
    draft.font().font_weight = values::font::FontWeight::from_float(650.0);
    draft.font().font_stretch = values::font::FontStretch::from_percentage(0.875);
    let lowered = lower::text_style(&draft.build());

    assert_eq!(lowered.size, CssPx(19.0));
    assert_eq!(lowered.weight, 650.0);
    assert!((lowered.width - 0.875).abs() < 1e-6);
}

/// The three forms of `line-height` stay apart, because two of them cannot be resolved yet.
#[test]
fn line_height_keeps_its_authored_form() {
    let plain = lower::text_style(&StyleDraft::initial().build());
    assert_eq!(plain.line_height, LineHeight::Normal);

    let mut draft = StyleDraft::initial();
    draft.font().line_height = line_height_number(1.5);
    assert_eq!(
        lower::text_style(&draft.build()).line_height,
        LineHeight::Number(1.5),
    );

    let mut draft = StyleDraft::initial();
    draft.font().line_height = line_height_length(CssPx(28.0));
    assert_eq!(
        lower::text_style(&draft.build()).line_height,
        LineHeight::Length(CssPx(28.0)),
    );
}

/// Spacing arrives with its percentage still unresolved, because the basis is not known here.
#[test]
fn spacing_keeps_its_percentage_unresolved() {
    let mut draft = StyleDraft::initial();
    draft.inherited_text().letter_spacing = letter_spacing(CssPx(1.5));
    draft.inherited_text().word_spacing = word_spacing(CssPx(3.0));
    let absolute = lower::text_style(&draft.build());

    assert_eq!(absolute.letter_spacing, LengthPercent::length(CssPx(1.5)));
    assert_eq!(absolute.word_spacing, LengthPercent::length(CssPx(3.0)));
    assert!(absolute.letter_spacing.is_absolute());

    let mut draft = StyleDraft::initial();
    draft.inherited_text().text_indent.length = values::length::percent(0.25);
    let indent = lower::paragraph_style(&draft.build()).indent;

    assert!(!indent.length.is_absolute(), "the percentage survived");
    assert_eq!(indent.length.resolve(CssPx(400.0)), CssPx(100.0));
}

/// Every wrapping enumeration maps onto its counterpart.
#[test]
fn the_wrapping_enumerations_map_across() {
    let cases = [
        (CascadedWordBreak::Normal, WordBreak::Normal),
        (CascadedWordBreak::BreakAll, WordBreak::BreakAll),
        (CascadedWordBreak::KeepAll, WordBreak::KeepAll),
    ];
    for (cascaded, expected) in cases {
        let mut draft = StyleDraft::initial();
        draft.inherited_text().word_break = cascaded;
        assert_eq!(lower::text_style(&draft.build()).word_break, expected);
    }

    let cases = [
        (CascadedOverflowWrap::Normal, OverflowWrap::Normal),
        (CascadedOverflowWrap::BreakWord, OverflowWrap::BreakWord),
        (CascadedOverflowWrap::Anywhere, OverflowWrap::Anywhere),
    ];
    for (cascaded, expected) in cases {
        let mut draft = StyleDraft::initial();
        draft.inherited_text().overflow_wrap = cascaded;
        assert_eq!(lower::text_style(&draft.build()).overflow_wrap, expected);
    }

    let cases = [
        (CascadedWhiteSpace::Collapse, WhiteSpaceCollapse::Collapse),
        (CascadedWhiteSpace::Preserve, WhiteSpaceCollapse::Preserve),
        (
            CascadedWhiteSpace::PreserveBreaks,
            WhiteSpaceCollapse::PreserveBreaks,
        ),
        (
            CascadedWhiteSpace::BreakSpaces,
            WhiteSpaceCollapse::BreakSpaces,
        ),
    ];
    for (cascaded, expected) in cases {
        let mut draft = StyleDraft::initial();
        draft.inherited_text().white_space_collapse = cascaded;
        assert_eq!(lower::text_style(&draft.build()).white_space, expected);
    }

    let mut draft = StyleDraft::initial();
    draft.inherited_text().text_wrap_mode = TextWrapMode::Nowrap;
    assert_eq!(
        lower::text_style(&draft.build()).wrap_mode,
        WrapMode::NoWrap
    );
}

/// `text-justify` maps across.
#[test]
fn justification_maps_across() {
    let cases = [
        (CascadedTextJustify::Auto, TextJustify::Auto),
        (CascadedTextJustify::None, TextJustify::None),
        (CascadedTextJustify::InterWord, TextJustify::InterWord),
        (
            CascadedTextJustify::InterCharacter,
            TextJustify::InterCharacter,
        ),
    ];
    for (cascaded, expected) in cases {
        let mut draft = StyleDraft::initial();
        draft.inherited_text().text_justify = cascaded;
        assert_eq!(lower::paragraph_style(&draft.build()).justify, expected);
    }
}

/// Alignment maps across, and the compatibility spellings fold onto the plain ones.
#[test]
fn alignment_maps_across_and_folds_the_prefixed_spellings() {
    let cases = [
        (TextAlignKeyword::Start, TextAlign::Start),
        (TextAlignKeyword::End, TextAlign::End),
        (TextAlignKeyword::Left, TextAlign::Left),
        (TextAlignKeyword::Right, TextAlign::Right),
        (TextAlignKeyword::Center, TextAlign::Center),
        (TextAlignKeyword::Justify, TextAlign::Justify),
        (TextAlignKeyword::MozLeft, TextAlign::Left),
        (TextAlignKeyword::MozRight, TextAlign::Right),
        (TextAlignKeyword::MozCenter, TextAlign::Center),
    ];
    for (cascaded, expected) in cases {
        let mut draft = StyleDraft::initial();
        draft.inherited_text().text_align = cascaded;
        assert_eq!(lower::paragraph_style(&draft.build()).align, expected);
    }
}

/// The base direction comes from the inherited-box group.
#[test]
fn the_base_direction_arrives() {
    let mut draft = StyleDraft::initial();
    draft.inherited_box().direction = CascadedDirection::Rtl;
    assert_eq!(
        lower::paragraph_style(&draft.build()).direction,
        Direction::RightToLeft,
    );
}

/// The writing mode comes from the inherited-box group too, and every keyword maps across.
#[test]
fn every_writing_mode_maps_across() {
    let cases = [
        (CascadedWritingMode::HorizontalTb, WritingMode::HorizontalTb),
        (CascadedWritingMode::VerticalRl, WritingMode::VerticalRl),
        (CascadedWritingMode::VerticalLr, WritingMode::VerticalLr),
    ];
    for (cascaded, expected) in cases {
        let mut draft = StyleDraft::initial();
        draft.inherited_box().writing_mode = cascaded;
        let lowered = lower::paragraph_style(&draft.build()).writing_mode;
        assert_eq!(lowered, expected);
        assert_eq!(lowered.is_vertical(), expected != WritingMode::HorizontalTb);
    }
}

/// Every `line-break` keyword maps across.
#[test]
fn every_line_break_keyword_maps_across() {
    let cases = [
        (CascadedLineBreak::Auto, LineBreak::Auto),
        (CascadedLineBreak::Loose, LineBreak::Loose),
        (CascadedLineBreak::Normal, LineBreak::Normal),
        (CascadedLineBreak::Strict, LineBreak::Strict),
        (CascadedLineBreak::Anywhere, LineBreak::Anywhere),
    ];
    for (cascaded, expected) in cases {
        let mut draft = StyleDraft::initial();
        draft.inherited_text().line_break = cascaded;
        assert_eq!(lower::text_style(&draft.build()).line_break, expected);
    }
}

/// Every `text-align-last` keyword maps across, and none of them folds onto another.
#[test]
fn every_text_align_last_keyword_maps_across() {
    let cases = [
        (CascadedAlignLast::Auto, TextAlignLast::Auto),
        (CascadedAlignLast::Start, TextAlignLast::Start),
        (CascadedAlignLast::End, TextAlignLast::End),
        (CascadedAlignLast::Left, TextAlignLast::Left),
        (CascadedAlignLast::Right, TextAlignLast::Right),
        (CascadedAlignLast::Center, TextAlignLast::Center),
        (CascadedAlignLast::Justify, TextAlignLast::Justify),
    ];
    for (cascaded, expected) in cases {
        let mut draft = StyleDraft::initial();
        draft.inherited_text().text_align_last = cascaded;
        assert_eq!(lower::paragraph_style(&draft.build()).align_last, expected);
    }
}

/// The three flag-set variant properties survive being split into independent choices.
///
/// A flag set can say several things at once, and the lowering answers each of them separately.
/// Asserting one flag at a time would pass on a lowering that read the whole set as a single
/// keyword and dropped everything but the first thing it recognised, so each case here sets two
/// flags from different groups and checks both.
#[test]
fn the_flag_set_variant_properties_split_into_their_groups() {
    let mut draft = StyleDraft::initial();
    draft.font().font_variant_numeric =
        CascadedNumeric::OLDSTYLE_NUMS | CascadedNumeric::TABULAR_NUMS | CascadedNumeric::ORDINAL;
    let numeric = lower::text_style(&draft.build()).variant.numeric;
    assert_eq!(numeric.figures, variant::NumericFigures::Oldstyle);
    assert_eq!(numeric.spacing, variant::NumericSpacing::Tabular);
    assert_eq!(numeric.fractions, variant::NumericFractions::Auto);
    assert!(numeric.ordinal);
    assert!(!numeric.slashed_zero);

    let mut draft = StyleDraft::initial();
    draft.font().font_variant_east_asian =
        CascadedEastAsian::TRADITIONAL | CascadedEastAsian::FULL_WIDTH;
    let east_asian = lower::text_style(&draft.build()).variant.east_asian;
    assert_eq!(east_asian.forms, variant::EastAsianForms::Traditional);
    assert_eq!(east_asian.width, variant::EastAsianWidth::FullWidth);
    assert!(!east_asian.ruby);

    let mut draft = StyleDraft::initial();
    draft.font().font_variant_ligatures =
        CascadedLigatures::NO_COMMON_LIGATURES | CascadedLigatures::DISCRETIONARY_LIGATURES;
    let ligatures = lower::text_style(&draft.build()).variant.ligatures;
    assert_eq!(ligatures.common, variant::LigatureSetting::Off);
    assert_eq!(ligatures.discretionary, variant::LigatureSetting::On);
    assert_eq!(ligatures.historical, variant::LigatureSetting::Auto);
    assert_eq!(ligatures.contextual, variant::LigatureSetting::Auto);
}

/// `font-variant-ligatures: none` is one flag standing for four, and is not the empty set.
///
/// Without the special case it would lower to four `Auto` settings, which asks for nothing and
/// leaves every ligature the face applies by default in place — the exact opposite of what was
/// written.
#[test]
fn the_none_ligature_keyword_turns_all_four_groups_off() {
    let mut draft = StyleDraft::initial();
    draft.font().font_variant_ligatures = CascadedLigatures::NONE;
    let ligatures = lower::text_style(&draft.build()).variant.ligatures;
    assert_eq!(ligatures, variant::FontVariantLigatures::none());
    assert_ne!(ligatures, variant::FontVariantLigatures::NORMAL);
}

/// The document language arrives when there is one and is absent when there is not.
#[test]
fn the_language_is_absent_until_the_document_declares_one() {
    assert_eq!(
        lower::text_style(&StyleDraft::initial().build()).language,
        None
    );

    let mut draft = StyleDraft::initial();
    draft.font()._x_lang = values::font::XLang(zgui_css::name::Atom::from("fr"));
    assert_eq!(
        lower::text_style(&draft.build()).language,
        Some(Ident::new("fr")),
    );
}

/// The colour is lowered separately, and is not in the text style at all.
#[test]
fn the_colour_is_lowered_beside_the_text_style_and_not_into_it() {
    let mut draft = StyleDraft::initial();
    draft.inherited_text().color = values::color::AbsoluteColor::srgb_legacy(255, 0, 0, 1.0);
    let style = draft.build();

    let set = lower::style_set(&style);
    assert_eq!(
        set.paint.color.to_premultiplied_srgb(),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(set.text, lower::text_style(&style));
}

/// A brush slot is claimed per cascade result, and two live results never claim the same slot.
///
/// The key is an address, and an address is only an identity while the allocation behind it is
/// alive. Under a key that was merely the number, the styles this loop builds are temporaries: each
/// is freed before the next is allocated, the allocator hands the block straight back, and a table
/// mapping key to slot would give a second colour the first colour's slot — every paragraph already
/// pointing at it silently re-coloured. Holding the paints is what makes the addresses distinct,
/// and the count is the proof.
#[test]
fn two_live_paints_from_different_cascade_results_never_share_a_key() {
    let paints: Vec<_> = (0..256u16)
        .map(|index| {
            let mut draft = StyleDraft::initial();
            draft.inherited_text().color =
                values::color::AbsoluteColor::srgb_legacy(index as u8, 0, 0, 1.0);
            lower::style_set(&draft.build()).paint
        })
        .collect();

    let distinct: std::collections::HashSet<_> = paints.iter().map(|paint| &paint.key).collect();
    assert_eq!(
        distinct.len(),
        paints.len(),
        "two live cascade results reported the same brush key, so a table on it is unsound",
    );
}

/// The other half of the same claim: one cascade result is *one* identity, not a fresh one each
/// time it is asked for. A key that were unique per call would claim a slot per element and defeat
/// the sharing the whole arrangement exists for.
#[test]
fn one_cascade_result_always_reports_the_same_brush_key() {
    let style = StyleDraft::initial().build();
    let first = lower::style_set(&style).paint;
    for _ in 0..64 {
        assert_eq!(lower::style_set(&style).paint.key, first.key);
    }
}

/// A key made of addresses is only an identity while the addresses cannot be reissued, so an entry
/// keeps the style it was made from alive.
///
/// Without that, a style dropped after being lowered frees its groups, the next style built lands
/// on the same addresses, and the cache answers it with the previous style's lowering — every
/// property wrong and nothing reported. The loop is what forces the reuse: the styles it builds are
/// temporaries, so each is freed before the next is allocated.
#[test]
fn a_freed_styles_address_is_never_answered_with_its_lowering() {
    let mut cache = TextStyleCache::default();

    for _ in 0..64 {
        let lowered = cache.get(&StyleDraft::initial().with_font_size(CssPx(11.0)).build());
        assert_eq!(lowered.text.size, CssPx(11.0));
    }
    for _ in 0..256 {
        let lowered = cache.get(&StyleDraft::initial().with_font_size(CssPx(37.0)).build());
        assert_eq!(
            lowered.text.size,
            CssPx(37.0),
            "a 37px style was answered with another style's lowering",
        );
    }
}

/// The cache lowers once per distinct set of property groups, not once per element.
#[test]
fn a_thousand_elements_sharing_one_style_lower_once() {
    let mut cache = TextStyleCache::default();
    let style = StyleDraft::initial().build();

    let first = cache.get(&style);
    for _ in 0..999 {
        let again = cache.get(&style);
        assert!(Arc::ptr_eq(&first, &again));
    }

    assert_eq!(cache.lowerings(), 1);
    assert_eq!(cache.hits(), 999);
    assert_eq!(cache.len(), 1);

    // A genuinely different style is a genuinely different entry.
    let mut draft = StyleDraft::from_style(&style);
    draft.font().font_size = FontSize::for_px(CssPx(11.0));
    cache.get(&draft.build());
    assert_eq!(cache.lowerings(), 2);
}
