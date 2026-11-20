//! The properties that change only what the engine animates.
//!
//! The `transition-*` properties tell the engine whether, and how, a later change of a value
//! animates. The `animation-*` properties tell it which keyframe animations run. Layout, paint and
//! the accessibility projection never read either group. What a change to them does is start, stop
//! or move an animation, and each of those changes the animated properties' own computed values:
//! a running transition whose property leaves `transition-property` jumps to its end, and a
//! keyframe animation writes its values in the cascade that samples it. The comparison of those
//! properties gives the damage for them, so a change to these alone costs nothing.

use style::properties::ComputedValues;

use crate::stylo::element::damage::classify::same::same;

/// Whether the animations and transitions declared on the element agree.
pub(super) fn unchanged(old: &ComputedValues, new: &ComputedValues) -> bool {
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
        ]
    )
}
