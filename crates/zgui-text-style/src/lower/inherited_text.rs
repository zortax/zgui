//! The inherited-text group: spacing, wrapping, alignment and indent.

use zgui_css::computed::style_structs;
use zgui_css::values::length::{LengthPercentage, evaluate_at};
use zgui_css::values::text;
use zgui_geom::CssPx;

use crate::style::paragraph::{TextAlign, TextAlignLast, TextIndent, TextJustify};
use crate::style::spacing::LengthPercent;
use crate::style::wrap::{LineBreak, OverflowWrap, WhiteSpaceCollapse, WordBreak, WrapMode};

/// Splits a cascaded length-or-percentage into its absolute and fractional parts.
///
/// The value is a function of one basis. For a plain length, a plain percentage and a sum of the
/// two — which is every value an author writes short of a comparison function — it is *affine* in
/// that basis, so evaluating it at two bases recovers both parts exactly: the value at a zero basis
/// is the absolute part, and the difference over a unit basis is the fraction.
///
/// `min()`, `max()` and `clamp()` are the exception, and the two-point reconstruction is an
/// approximation of them rather than an answer: it takes whichever branch each of the two probes
/// happened to select. Resolving them faithfully needs the basis, which is precisely what is not
/// known here, so it would have to be an unevaluated expression carried through to the shaper.
fn split(value: &LengthPercentage) -> LengthPercent {
    let at_zero = evaluate_at(value, CssPx(0.0));
    let at_one = evaluate_at(value, CssPx(1.0));
    LengthPercent {
        length: at_zero,
        percent: at_one.0 - at_zero.0,
    }
}

/// `letter-spacing`.
pub fn letter_spacing(group: &style_structs::InheritedText) -> LengthPercent {
    split(&group.letter_spacing.0)
}

/// `word-spacing`.
pub fn word_spacing(group: &style_structs::InheritedText) -> LengthPercent {
    split(&group.word_spacing)
}

/// `word-break`.
pub fn word_break(group: &style_structs::InheritedText) -> WordBreak {
    match group.word_break {
        text::WordBreak::Normal => WordBreak::Normal,
        text::WordBreak::BreakAll => WordBreak::BreakAll,
        text::WordBreak::KeepAll => WordBreak::KeepAll,
    }
}

/// `overflow-wrap`.
pub fn overflow_wrap(group: &style_structs::InheritedText) -> OverflowWrap {
    match group.overflow_wrap {
        text::OverflowWrap::Normal => OverflowWrap::Normal,
        text::OverflowWrap::BreakWord => OverflowWrap::BreakWord,
        text::OverflowWrap::Anywhere => OverflowWrap::Anywhere,
    }
}

/// `line-break`.
pub fn line_break(group: &style_structs::InheritedText) -> LineBreak {
    match group.line_break {
        text::LineBreak::Auto => LineBreak::Auto,
        text::LineBreak::Loose => LineBreak::Loose,
        text::LineBreak::Normal => LineBreak::Normal,
        text::LineBreak::Strict => LineBreak::Strict,
        text::LineBreak::Anywhere => LineBreak::Anywhere,
    }
}

/// `text-wrap-mode`.
pub fn wrap_mode(group: &style_structs::InheritedText) -> WrapMode {
    match group.text_wrap_mode {
        text::TextWrapMode::Wrap => WrapMode::Wrap,
        text::TextWrapMode::Nowrap => WrapMode::NoWrap,
    }
}

/// `white-space-collapse`.
pub fn white_space(group: &style_structs::InheritedText) -> WhiteSpaceCollapse {
    match group.white_space_collapse {
        text::WhiteSpaceCollapse::Collapse => WhiteSpaceCollapse::Collapse,
        text::WhiteSpaceCollapse::Preserve => WhiteSpaceCollapse::Preserve,
        text::WhiteSpaceCollapse::PreserveBreaks => WhiteSpaceCollapse::PreserveBreaks,
        text::WhiteSpaceCollapse::BreakSpaces => WhiteSpaceCollapse::BreakSpaces,
    }
}

/// `text-align`, with the compatibility spellings folded onto the values they behave as.
///
/// The three prefixed keywords exist so that a legacy `align` attribute keeps working. Each
/// differs from its unprefixed spelling only in how it centres *block-level* boxes, which is not a
/// question a line of text asks, so a run treats them as the plain value.
pub fn align(group: &style_structs::InheritedText) -> TextAlign {
    match group.text_align {
        text::TextAlignKeyword::Start => TextAlign::Start,
        text::TextAlignKeyword::End => TextAlign::End,
        text::TextAlignKeyword::Left | text::TextAlignKeyword::MozLeft => TextAlign::Left,
        text::TextAlignKeyword::Right | text::TextAlignKeyword::MozRight => TextAlign::Right,
        text::TextAlignKeyword::Center | text::TextAlignKeyword::MozCenter => TextAlign::Center,
        text::TextAlignKeyword::Justify => TextAlign::Justify,
    }
}

/// `text-align-last`.
pub fn align_last(group: &style_structs::InheritedText) -> TextAlignLast {
    match group.text_align_last {
        text::TextAlignLast::Auto => TextAlignLast::Auto,
        text::TextAlignLast::Start => TextAlignLast::Start,
        text::TextAlignLast::End => TextAlignLast::End,
        text::TextAlignLast::Left => TextAlignLast::Left,
        text::TextAlignLast::Right => TextAlignLast::Right,
        text::TextAlignLast::Center => TextAlignLast::Center,
        text::TextAlignLast::Justify => TextAlignLast::Justify,
    }
}

/// `text-justify`.
pub fn justify(group: &style_structs::InheritedText) -> TextJustify {
    match group.text_justify {
        text::TextJustify::Auto => TextJustify::Auto,
        text::TextJustify::None => TextJustify::None,
        text::TextJustify::InterWord => TextJustify::InterWord,
        text::TextJustify::InterCharacter => TextJustify::InterCharacter,
    }
}

/// `text-indent`.
pub fn indent(group: &style_structs::InheritedText) -> TextIndent {
    TextIndent {
        length: split(&group.text_indent.length),
        hanging: group.text_indent.hanging,
        each_line: group.text_indent.each_line,
    }
}
