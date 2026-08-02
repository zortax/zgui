//! What each half of the split is allowed to notice.
//!
//! The claim under test is the one every text cache rests on: a change that leaves the shaping key
//! alone can never require a fresh shape. Each test therefore does two things — asserts the key
//! that must not move has not moved, *and* asserts that the two styles genuinely differ, so that a
//! lowering which silently dropped the property could not make the test pass.

mod support;

use support::{Vary, pair, seed};
use zgui_css::values::font::{FontSize, FontSizeExt};
use zgui_css::values::text::{TextAlignKeyword, TextWrapMode};
use zgui_css::{StyleDraft, values};
use zgui_geom::CssPx;
use zgui_text_style::{BreakingKey, ShapingKey, TextDamage, lower};

/// A colour change must move neither key: a run's paint is an index into a table the shaped result
/// does not own, so re-theming rewrites the table and leaves every shaped paragraph valid.
#[test]
fn five_hundred_colour_pairs_hash_to_the_same_two_keys() {
    let colours: Vec<[u8; 3]> = (0..64u32)
        .map(|index| {
            let word = seed(index);
            [word as u8, (word >> 8) as u8, (word >> 16) as u8]
        })
        .collect();

    for index in 0..500u32 {
        let Vary { before, after } = pair(index, &colours, |draft, [red, green, blue]| {
            draft.inherited_text().color =
                values::color::AbsoluteColor::srgb_legacy(red, green, blue, 1.0);
        });

        // The pair really does differ, which is what stops this test from passing vacuously: a
        // lowering that dropped the colour entirely would satisfy every assertion below.
        assert_ne!(
            lower::style_set(&before).paint.color,
            lower::style_set(&after).paint.color,
            "the two styles must genuinely differ in colour",
        );

        assert_eq!(
            ShapingKey::of(&lower::text_style(&before)),
            ShapingKey::of(&lower::text_style(&after)),
            "colour must not reach the shaping key",
        );
        assert_eq!(
            BreakingKey::of(&lower::text_style(&before)),
            BreakingKey::of(&lower::text_style(&after)),
            "colour must not reach the breaking key",
        );
        assert_eq!(
            BreakingKey::of_paragraph(&lower::paragraph_style(&before)),
            BreakingKey::of_paragraph(&lower::paragraph_style(&after)),
        );
        assert_eq!(TextDamage::between(&before, &after), TextDamage::None);
    }
}

/// Letter spacing is baked into cluster advances, so it is a shaping change.
#[test]
fn five_hundred_letter_spacing_pairs_move_the_shaping_key() {
    let spacings: Vec<f32> = (0..64u32).map(|step| 0.25 + step as f32 / 8.0).collect();
    for index in 0..500u32 {
        let Vary { before, after } = pair(index, &spacings, |draft, spacing| {
            draft.inherited_text().letter_spacing = values::text::letter_spacing(CssPx(spacing));
        });
        assert_ne!(
            lower::text_style(&before).letter_spacing,
            lower::text_style(&after).letter_spacing,
            "the two styles must genuinely differ in letter spacing",
        );
        assert_ne!(
            ShapingKey::of(&lower::text_style(&before)),
            ShapingKey::of(&lower::text_style(&after)),
            "letter spacing must move the shaping key",
        );
        assert!(TextDamage::between(&before, &after).reshapes());
    }
}

/// Alignment moves no glyph, so it must move the breaking key and only the breaking key.
#[test]
fn five_hundred_text_align_pairs_move_only_the_breaking_key() {
    let keywords = [
        TextAlignKeyword::Center,
        TextAlignKeyword::End,
        TextAlignKeyword::Justify,
        TextAlignKeyword::Left,
        TextAlignKeyword::Right,
    ];
    for index in 0..500u32 {
        let Vary { before, after } = pair(index, &keywords, |draft, keyword| {
            draft.inherited_text().text_align = keyword;
        });
        assert_ne!(
            lower::paragraph_style(&before).align,
            lower::paragraph_style(&after).align,
            "the two styles must genuinely differ in alignment",
        );
        assert_eq!(
            ShapingKey::of(&lower::text_style(&before)),
            ShapingKey::of(&lower::text_style(&after)),
            "alignment must not move the shaping key",
        );
        assert_ne!(
            BreakingKey::of_paragraph(&lower::paragraph_style(&before)),
            BreakingKey::of_paragraph(&lower::paragraph_style(&after)),
            "alignment must move the breaking key",
        );
        assert_eq!(TextDamage::between(&before, &after), TextDamage::Rebreak);
    }
}

/// Every property the shaper bakes into cluster advances moves the shaping key.
#[test]
fn shaping_relevant_changes_move_the_shaping_key() {
    let base = StyleDraft::initial().build();
    let reference = ShapingKey::of(&lower::text_style(&base));

    let mut moved = Vec::new();
    for (name, apply) in shaping_variants() {
        let mut draft = StyleDraft::from_style(&base);
        apply(&mut draft);
        let varied = draft.build();
        assert_ne!(
            ShapingKey::of(&lower::text_style(&varied)),
            reference,
            "{name} must move the shaping key",
        );
        moved.push(name);
    }
    assert_eq!(moved.len(), 8, "every listed property was exercised");
}

/// The breaking half moves the breaking key and leaves the shaping key alone.
#[test]
fn breaking_relevant_changes_keep_the_shaping_key() {
    let base = StyleDraft::initial().build();
    let shaping = ShapingKey::of(&lower::text_style(&base));
    let breaking = BreakingKey::of(&lower::text_style(&base));

    for (name, apply) in breaking_variants() {
        let mut draft = StyleDraft::from_style(&base);
        apply(&mut draft);
        let varied = draft.build();
        assert_eq!(
            ShapingKey::of(&lower::text_style(&varied)),
            shaping,
            "{name} must not move the shaping key",
        );
        assert_ne!(
            BreakingKey::of(&lower::text_style(&varied)),
            breaking,
            "{name} must move the breaking key",
        );
        assert_eq!(TextDamage::between(&base, &varied), TextDamage::Rebreak);
    }
}

/// Two styles built the same way hash the same, in this process and in the next one.
#[test]
fn the_same_style_hashes_to_the_same_key_every_time() {
    let one = StyleDraft::initial().with_font_size(CssPx(17.5)).build();
    let other = StyleDraft::initial().with_font_size(CssPx(17.5)).build();
    assert_eq!(
        ShapingKey::of(&lower::text_style(&one)),
        ShapingKey::of(&lower::text_style(&other)),
    );
    assert_eq!(TextDamage::between(&one, &other), TextDamage::None);
}

/// The face and spacing properties, each varied on its own.
///
/// Not the whole shaping half: the properties that select among a face's optional substitutions
/// have a test each in `damage.rs`, which asserts more about them than "a key moved".
#[allow(clippy::type_complexity)]
fn shaping_variants() -> Vec<(&'static str, Box<dyn Fn(&mut StyleDraft)>)> {
    vec![
        (
            "font-size",
            Box::new(|draft: &mut StyleDraft| {
                draft.font().font_size = FontSize::for_px(CssPx(24.0));
            }),
        ),
        (
            "font-weight",
            Box::new(|draft: &mut StyleDraft| {
                draft.font().font_weight = values::font::FontWeight::from_float(700.0);
            }),
        ),
        (
            "font-style",
            Box::new(|draft: &mut StyleDraft| {
                draft.font().font_style = values::font::FontStyle::ITALIC;
            }),
        ),
        (
            "font-width",
            Box::new(|draft: &mut StyleDraft| {
                draft.font().font_stretch = values::font::FontStretch::from_percentage(0.75);
            }),
        ),
        (
            "line-height",
            Box::new(|draft: &mut StyleDraft| {
                draft.font().line_height = values::font::line_height_number(1.5);
            }),
        ),
        (
            "word-spacing",
            Box::new(|draft: &mut StyleDraft| {
                draft.inherited_text().word_spacing = values::text::word_spacing(CssPx(2.0));
            }),
        ),
        (
            "word-break",
            Box::new(|draft: &mut StyleDraft| {
                draft.inherited_text().word_break = values::text::WordBreak::BreakAll;
            }),
        ),
        (
            "white-space-collapse",
            Box::new(|draft: &mut StyleDraft| {
                draft.inherited_text().white_space_collapse =
                    values::text::WhiteSpaceCollapse::Preserve;
            }),
        ),
    ]
}

/// The wrapping properties, each varied on its own. `line-break` has its own test in `damage.rs`.
#[allow(clippy::type_complexity)]
fn breaking_variants() -> Vec<(&'static str, Box<dyn Fn(&mut StyleDraft)>)> {
    vec![
        (
            "overflow-wrap",
            Box::new(|draft: &mut StyleDraft| {
                draft.inherited_text().overflow_wrap = values::text::OverflowWrap::BreakWord;
            }),
        ),
        (
            "text-wrap-mode",
            Box::new(|draft: &mut StyleDraft| {
                draft.inherited_text().text_wrap_mode = TextWrapMode::Nowrap;
            }),
        ),
    ]
}

/// The base direction changes the visual order of the runs and mirrors bracket characters, so it is
/// a shaping change even though it is a paragraph-level property.
#[test]
fn the_base_direction_is_a_shaping_change() {
    let before = StyleDraft::initial().build();
    let mut draft = StyleDraft::from_style(&before);
    draft.inherited_box().direction = values::text::Direction::Rtl;
    let after = draft.build();

    assert_ne!(
        lower::paragraph_style(&before).direction,
        lower::paragraph_style(&after).direction,
    );
    assert_ne!(
        ShapingKey::of_paragraph(&lower::paragraph_style(&before)),
        ShapingKey::of_paragraph(&lower::paragraph_style(&after)),
    );
    assert_eq!(
        BreakingKey::of_paragraph(&lower::paragraph_style(&before)),
        BreakingKey::of_paragraph(&lower::paragraph_style(&after)),
        "and it is not counted twice, on the breaking side as well",
    );
    assert_eq!(TextDamage::between(&before, &after), TextDamage::Reshape);
}

/// A change to a property the text pipeline does not read costs nothing at all.
#[test]
fn a_change_the_text_pipeline_does_not_read_costs_nothing() {
    let before = StyleDraft::initial().build();
    let mut draft = StyleDraft::from_style(&before);
    draft.inherited_text().color = values::color::AbsoluteColor::srgb_legacy(9, 9, 9, 1.0);
    let after = draft.build();

    assert_eq!(TextDamage::between(&before, &after), TextDamage::None);
    assert!(!TextDamage::between(&before, &after).rebreaks());
}

/// `text-justify` decides what a justified line stretches, which changes how much fits on it — so
/// it is a breaking change and not a free one.
#[test]
fn text_justify_costs_a_break_and_no_shape() {
    let before = StyleDraft::initial().build();
    let mut draft = StyleDraft::from_style(&before);
    draft.inherited_text().text_justify = values::text::TextJustify::InterCharacter;
    let after = draft.build();

    assert_ne!(
        lower::paragraph_style(&before).justify,
        lower::paragraph_style(&after).justify,
        "the two styles must genuinely differ",
    );
    assert_eq!(
        ShapingKey::of_paragraph(&lower::paragraph_style(&before)),
        ShapingKey::of_paragraph(&lower::paragraph_style(&after)),
        "it must not move the shaping key",
    );
    assert_eq!(TextDamage::between(&before, &after), TextDamage::Rebreak);
}
