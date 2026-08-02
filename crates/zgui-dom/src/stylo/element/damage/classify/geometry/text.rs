//! The properties that decide how text is shaped and where it breaks.

use style::properties::ComputedValues;

use crate::stylo::element::damage::classify::same::same;

/// Whether nothing that changes a shaped run or a line break moved.
pub(super) fn unchanged(old: &ComputedValues, new: &ComputedValues) -> bool {
    font(old, new) && flow(old, new)
}

/// Whether the face and the size the text is shaped at agree.
fn font(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_font,
        [
            _x_lang,
            font_family,
            font_feature_settings,
            font_kerning,
            font_language_override,
            font_optical_sizing,
            font_size,
            font_stretch,
            font_style,
            font_synthesis_weight,
            font_variant_caps,
            font_variant_east_asian,
            font_variant_ligatures,
            font_variant_numeric,
            font_variant_position,
            font_variation_settings,
            font_weight,
            line_height,
        ]
    )
}

/// Whether the rules that break a run into lines and place them agree.
fn flow(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_inherited_text,
        [
            letter_spacing,
            line_break,
            overflow_wrap,
            tab_size,
            text_align,
            text_align_last,
            text_indent,
            text_justify,
            text_rendering,
            text_transform,
            text_wrap_mode,
            white_space_collapse,
            word_break,
            word_spacing,
            _webkit_text_security,
        ]
    ) && same!(old, new, get_text, [text_overflow, unicode_bidi])
        && same!(
            old,
            new,
            get_inherited_box,
            [direction, visibility, writing_mode]
        )
}
