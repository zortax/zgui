//! The three results of lowering one style, and the whole-style entry points.

use zgui_css::ComputedStyle;

use crate::lower::{font, inherited_box, inherited_text, paint, variant};
use crate::style::paint::TextPaint;
use crate::style::paragraph::ParagraphStyle;
use crate::style::text::TextStyle;

/// Everything one cascaded style contributes to laying out and drawing text.
///
/// The three parts are produced together because they come from the same three property groups, and
/// they are kept apart because they have different lifetimes: the run style is what a shaped result
/// is cached against, the paragraph style is what a break is cached against, and the paint is what a
/// theme change rewrites without disturbing either.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyleSet {
    /// What one run of text shapes and breaks as.
    pub text: TextStyle,
    /// What the inline formatting context as a whole breaks as.
    pub paragraph: ParagraphStyle,
    /// What the text is drawn in.
    pub paint: TextPaint,
}

/// Lowers one style into all three parts.
pub fn style_set(style: &ComputedStyle) -> TextStyleSet {
    TextStyleSet {
        text: text_style(style),
        paragraph: paragraph_style(style),
        paint: paint::paint(style),
    }
}

/// Lowers the run half of one style.
pub fn text_style(style: &ComputedStyle) -> TextStyle {
    let font_group = style.get_font();
    let text_group = style.get_inherited_text();
    TextStyle {
        family: font::family(font_group),
        size: font::size(font_group),
        weight: font::weight(font_group),
        slant: font::slant(font_group),
        width: font::width(font_group),
        synthesis_weight: font::synthesis_weight(font_group),
        optical_sizing: font::optical_sizing(font_group),
        variations: font::variations(font_group),
        features: font::features(font_group),
        variant: variant::variant(font_group),
        language: font::language(font_group),
        language_system: font::language_system(font_group),
        letter_spacing: inherited_text::letter_spacing(text_group),
        word_spacing: inherited_text::word_spacing(text_group),
        line_height: font::line_height(font_group),
        word_break: inherited_text::word_break(text_group),
        white_space: inherited_text::white_space(text_group),
        overflow_wrap: inherited_text::overflow_wrap(text_group),
        wrap_mode: inherited_text::wrap_mode(text_group),
        line_break: inherited_text::line_break(text_group),
    }
}

/// Lowers the paragraph half of one style.
pub fn paragraph_style(style: &ComputedStyle) -> ParagraphStyle {
    let text_group = style.get_inherited_text();
    let box_group = style.get_inherited_box();
    ParagraphStyle {
        direction: inherited_box::direction(box_group),
        writing_mode: inherited_box::writing_mode(box_group),
        align: inherited_text::align(text_group),
        align_last: inherited_text::align_last(text_group),
        justify: inherited_text::justify(text_group),
        indent: inherited_text::indent(text_group),
    }
}
