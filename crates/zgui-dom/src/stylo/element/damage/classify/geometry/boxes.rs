//! The properties that decide where a box lands and how large it is.

use style::properties::ComputedValues;

use crate::stylo::element::damage::classify::same::same;

/// Whether nothing that sizes or places a box moved.
pub(super) fn unchanged(old: &ComputedValues, new: &ComputedValues) -> bool {
    sizes(old, new) && spacing(old, new)
}

/// Whether the properties that decide an element's own extent and position agree.
///
/// `order` and the three grid *templates* are absent on purpose: they are read while the box tree
/// is built rather than while it is measured, so they belong to
/// [`structure`](crate::stylo::element::damage::classify::structure).
fn sizes(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_position,
        [
            align_content,
            align_items,
            align_self,
            aspect_ratio,
            bottom,
            column_gap,
            flex_basis,
            flex_grow,
            flex_shrink,
            flex_direction,
            flex_wrap,
            box_sizing,
            object_fit,
            object_position,
            grid_auto_columns,
            grid_auto_flow,
            grid_auto_rows,
            grid_column_end,
            grid_column_start,
            grid_row_end,
            grid_row_start,
            height,
            justify_content,
            justify_items,
            justify_self,
            left,
            max_height,
            max_width,
            min_height,
            min_width,
            position_area,
            position_try_fallbacks,
            right,
            row_gap,
            top,
            width,
        ]
    )
}

/// Whether the space an element reserves around and inside itself agrees.
///
/// A border's *width* and *style* belong here and its colour does not: a style of `none` is a used
/// width of zero, so both change the box, while no colour ever has.
fn spacing(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_margin,
        [
            overflow_clip_margin,
            margin_top,
            margin_right,
            margin_bottom,
            margin_left,
        ]
    ) && same!(
        old,
        new,
        get_padding,
        [padding_top, padding_right, padding_bottom, padding_left]
    ) && same!(
        old,
        new,
        get_border,
        [
            border_top_style,
            border_right_style,
            border_bottom_style,
            border_left_style,
            border_top_width,
            border_right_width,
            border_bottom_width,
            border_left_width,
        ]
    )
}
