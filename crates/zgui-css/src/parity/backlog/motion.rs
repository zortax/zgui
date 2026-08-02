//! Animation, transition and interaction properties nothing has claimed yet.
//!
//! Every row here parses and cascades: an author may write it and the value reaches the
//! computed style. What none of them has is a reader — and that is not asserted, it is
//! measured: each one has a probe that sets it on a fixture, and none of those probes moves
//! anything the fragment tree or hit testing shows. A row that starts moving something fails.

use crate::parity::support::Support;

/// Why none of these has an effect yet.
const NOTE: &str = "nothing animates yet, so nothing reads it";

crate::register_properties! {
    animation_composition => Support::Ignored(NOTE),
    animation_delay => Support::Ignored(NOTE),
    animation_direction => Support::Ignored(NOTE),
    animation_duration => Support::Ignored(NOTE),
    animation_fill_mode => Support::Ignored(NOTE),
    animation_iteration_count => Support::Ignored(NOTE),
    animation_name => Support::Ignored(NOTE),
    animation_play_state => Support::Ignored(NOTE),
    animation_range_end => Support::Ignored(NOTE),
    animation_range_start => Support::Ignored(NOTE),
    animation_timeline => Support::Ignored(NOTE),
    animation_timing_function => Support::Ignored(NOTE),
    transition_behavior => Support::Ignored(NOTE),
    transition_delay => Support::Ignored(NOTE),
    transition_duration => Support::Ignored(NOTE),
    transition_property => Support::Ignored(NOTE),
    transition_timing_function => Support::Ignored(NOTE),
    user_select => Support::Ignored(NOTE),
}
