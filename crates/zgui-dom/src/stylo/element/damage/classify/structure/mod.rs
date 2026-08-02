//! The properties that decide which boxes an element generates, and how they nest.
//!
//! A change to any of these is the one kind of change the box tree cannot absorb: the tree that is
//! there describes a document that no longer exists, so the boxes have to be built again from the
//! element down. Everything else a style can say about layout is a number the boxes already there
//! are measured with, and belongs to [`geometry`](super::geometry) instead.
//!
//! The line between the two is drawn by asking what the box *builder* reads, not by asking what
//! sounds structural. `grid-template-columns` is here because the builder resolves its line names
//! into the container's box and nothing refreshes them afterwards; `grid-column-start` is not,
//! because it is read from the style every time the grid is sized.

mod boxes;
mod generated;

use style::properties::ComputedValues;

use crate::stylo::element::damage::classify::same::same;

/// Whether nothing that shapes the box tree moved.
pub(super) fn unchanged(old: &ComputedValues, new: &ComputedValues) -> bool {
    boxes::unchanged(old, new) && generated::unchanged(old, new) && timing(old, new)
}

/// Whether the animations and transitions declared on the element agree.
///
/// They shape no box at all, and they are here because starting or stopping one is a change to what
/// the element will do next rather than to how it looks now — which is not a thing this
/// classification is able to describe, and the widest answer is the safe one for a change it cannot
/// describe.
fn timing(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_ui,
        [
            animation_composition,
            animation_delay,
            animation_direction,
            animation_duration,
            animation_fill_mode,
            animation_iteration_count,
            animation_name,
            animation_play_state,
            animation_range_end,
            animation_range_start,
            animation_timeline,
            animation_timing_function,
            transition_behavior,
            transition_delay,
            transition_duration,
            transition_property,
            transition_timing_function,
            user_select,
            view_transition_class,
            view_transition_name,
        ]
    )
}
