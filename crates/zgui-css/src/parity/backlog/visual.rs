//! Painting properties nothing has claimed yet.
//!
//! Every row here parses and cascades: an author may write it and the value reaches the
//! computed style. What none of them has is a reader — and that is not asserted, it is
//! measured: each one has a probe that sets it on a fixture, and none of those probes moves
//! anything the fragment tree, the lowered painting or hit testing shows. A row that starts moving
//! something fails.
//!
//! The background, border, outline and mask groups are shorter than they look, because the paint
//! stage claims its own rows beside the code that lowers them. What is left here is the part of the
//! painting vocabulary no lowering names at all.

use crate::parity::support::Support;

/// Why none of these has an effect yet.
const NOTE: &str = "nothing paints yet, so nothing reads it";

crate::register_properties! {
    clip => Support::Ignored(NOTE),
    corner_bottom_left_shape => Support::Ignored(NOTE),
    corner_bottom_right_shape => Support::Ignored(NOTE),
    corner_end_end_shape => Support::Ignored(NOTE),
    corner_end_start_shape => Support::Ignored(NOTE),
    corner_start_end_shape => Support::Ignored(NOTE),
    corner_start_start_shape => Support::Ignored(NOTE),
    corner_top_left_shape => Support::Ignored(NOTE),
    corner_top_right_shape => Support::Ignored(NOTE),
    counter_increment => Support::Ignored(NOTE),
    counter_reset => Support::Ignored(NOTE),
    list_style_image => Support::Ignored(NOTE),
    list_style_position => Support::Ignored(NOTE),
    list_style_type => Support::Ignored(NOTE),
    mask_clip => Support::Ignored(NOTE),
    mask_composite => Support::Ignored(NOTE),
    mask_image => Support::Ignored(NOTE),
    mask_mode => Support::Ignored(NOTE),
    mask_origin => Support::Ignored(NOTE),
    mask_position_x => Support::Ignored(NOTE),
    mask_position_y => Support::Ignored(NOTE),
    mask_repeat => Support::Ignored(NOTE),
    mask_size => Support::Ignored(NOTE),
    mask_type => Support::Ignored(NOTE),
    quotes => Support::Ignored(NOTE),
}
