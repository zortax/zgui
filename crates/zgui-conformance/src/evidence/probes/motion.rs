//! Probes for the properties that describe change over time, and for the rest of the user-interface group.
//!
//! Each row names one longhand and a declaration that sets it to something other than its
//! initial value. The declaration is data, not a claim: what it is for is to be applied to a
//! fixture so that the framework can be asked whether anything downstream noticed.

use crate::evidence::probe::Probe;

/// One probe per longhand in this group.
pub static PROBES: &[Probe] = &[
    Probe::new("animation_composition", "animation-composition: add"),
    Probe::new("animation_delay", "animation-delay: 3s"),
    Probe::new("animation_direction", "animation-direction: reverse"),
    Probe::new("animation_duration", "animation-duration: 4s"),
    Probe::new("animation_fill_mode", "animation-fill-mode: both"),
    Probe::new("animation_iteration_count", "animation-iteration-count: 3"),
    Probe::new("animation_name", "animation-name: spin"),
    Probe::new("animation_play_state", "animation-play-state: paused"),
    Probe::new("animation_range_end", "animation-range-end: 50%"),
    Probe::new("animation_range_start", "animation-range-start: 25%"),
    Probe::new("animation_timeline", "animation-timeline: none"),
    Probe::new(
        "animation_timing_function",
        "animation-timing-function: linear",
    ),
    Probe::new("transition_behavior", "transition-behavior: allow-discrete"),
    Probe::new("transition_delay", "transition-delay: 3s"),
    Probe::new("transition_duration", "transition-duration: 4s"),
    Probe::new("transition_property", "transition-property: opacity"),
    Probe::new(
        "transition_timing_function",
        "transition-timing-function: linear",
    ),
    Probe::new("user_select", "user-select: none"),
];
