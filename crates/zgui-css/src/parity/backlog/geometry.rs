//! Sizing, spacing and containment properties nothing has claimed yet.
//!
//! Every row here parses and cascades: an author may write it and the value reaches the
//! computed style. What none of them has is a reader — and that is not asserted, it is
//! measured: each one has a probe that sets it on a fixture, and none of those probes moves
//! anything the fragment tree or hit testing shows. A row that starts moving something fails.

use crate::parity::support::Support;

/// Why none of these has an effect yet.
const NOTE: &str = "no probe has shown it moving an edge, and no module reads it";

crate::register_properties! {
    alignment_baseline => Support::Ignored(NOTE),
    baseline_shift => Support::Ignored(NOTE),
    baseline_source => Support::Ignored(NOTE),
    border_collapse => Support::Ignored(NOTE),
    border_spacing => Support::Ignored(NOTE),
    caption_side => Support::Ignored(NOTE),
    column_count => Support::Ignored(NOTE),
    column_span => Support::Ignored(NOTE),
    column_width => Support::Ignored(NOTE),
    contain => Support::Ignored(NOTE),
    container_name => Support::Ignored(NOTE),
    container_type => Support::Ignored(NOTE),
    empty_cells => Support::Ignored(NOTE),
    object_fit => Support::Ignored(NOTE),
    object_position => Support::Ignored(NOTE),
    offset_path => Support::Ignored(NOTE),
    overflow_clip_margin => Support::Ignored(NOTE),
    position_area => Support::Ignored(NOTE),
    position_try_fallbacks => Support::Ignored(NOTE),
    table_layout => Support::Ignored(NOTE),
    will_change => Support::Ignored(NOTE),
}
