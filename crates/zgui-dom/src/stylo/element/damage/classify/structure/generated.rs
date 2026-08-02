//! The properties that decide which boxes an element generates rather than contains.

use style::properties::ComputedValues;

use crate::stylo::element::damage::classify::same::same;

/// Whether the boxes an element generates rather than contains agree.
///
/// A `content` string is copied into the box built for it and nothing copies it again, a list
/// item's mark is a box of its own, and a column count, a table layout and a border-collapse model
/// each decide how many boxes a container's children are distributed into.
pub(super) fn unchanged(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_counters,
        [content, counter_increment, counter_reset]
    ) && same!(
        old,
        new,
        get_list,
        [
            list_style_image,
            list_style_type,
            quotes,
            list_style_position
        ]
    ) && same!(
        old,
        new,
        get_column,
        [column_count, column_width, column_span]
    ) && same!(old, new, get_table, [table_layout])
        && same!(
            old,
            new,
            get_inherited_table,
            [border_spacing, caption_side, border_collapse, empty_cells]
        )
}
