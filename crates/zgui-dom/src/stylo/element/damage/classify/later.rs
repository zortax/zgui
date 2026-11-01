//! The properties that change only what a later change does.
//!
//! The `transition-*` properties tell the engine whether, and how, a later change of a value
//! animates. Layout, paint and the accessibility projection never read them. A change to them has
//! one effect now: a running transition whose property leaves `transition-property` stops, and its
//! value jumps to the end. That jump is a change of the animated property's own computed value in
//! the same cascade, so the comparison of that property gives the damage for it.

use style::properties::ComputedValues;

use crate::stylo::element::damage::classify::same::same;

/// Whether the transitions declared on the element agree.
pub(super) fn unchanged(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_ui,
        [
            transition_behavior,
            transition_delay,
            transition_duration,
            transition_property,
            transition_timing_function,
        ]
    )
}
