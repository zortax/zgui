//! The properties that decide what kind of box an element is and where it is written.

use style::properties::ComputedValues;

use crate::stylo::element::damage::classify::same::same;

/// Whether the properties that decide what kind of box an element is, what contains it and where it
/// sits among its siblings agree.
pub(super) fn unchanged(old: &ComputedValues, new: &ComputedValues) -> bool {
    kind(old, new) && placement(old, new) && grid_names(old, new)
}

/// Whether the properties that decide what kind of box an element is, and what contains it, agree.
///
/// The transform test is not a property comparison: a transform is a containing block for
/// positioned descendants, so it is *whether there is one* that changes the tree rather than what
/// it is, and a rotation changing by two degrees moves nothing.
fn kind(old: &ComputedValues, new: &ComputedValues) -> bool {
    let unmoved = same!(
        old,
        new,
        get_box,
        [
            alignment_baseline,
            baseline_shift,
            baseline_source,
            clear,
            contain,
            container_name,
            container_type,
            display,
            original_display,
            float,
            offset_path,
            overflow_x,
            overflow_y,
            position,
            will_change,
            zoom,
            _servo_top_layer,
        ]
    );
    unmoved
        && old.get_box().has_transform_or_perspective()
            == new.get_box().has_transform_or_perspective()
        && old.get_effects().filter.0.is_empty() == new.get_effects().filter.0.is_empty()
}

/// Whether where the box is written among its siblings agrees.
///
/// `order` is not a number the layout reads at the moment it lays a container out: the builder
/// sorts a flex or grid container's layout children by it while the tree is made, so a box whose
/// `order` moved is in the wrong place in a list nothing else rewrites.
fn placement(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(old, new, get_position, [order])
}

/// Whether the grid line and area *names* a container declares agree.
///
/// Only the names, and only on the container. They are resolved into the container's box while the
/// tree is built, because resolving a name costs an allocation and every track-sizing pass would
/// otherwise repeat it — so a template whose names moved leaves the box holding names for a grid
/// that is not there. Every other grid property, including the track sizes in these very same
/// declarations, is read from the style each time the grid is sized and belongs to
/// [`geometry`](crate::stylo::element::damage::classify::geometry).
fn grid_names(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_position,
        [
            grid_template_areas,
            grid_template_columns,
            grid_template_rows
        ]
    )
}
