//! Painting properties nothing has claimed yet.
//!
//! Every row here parses and cascades: an author may write it and the value reaches the
//! computed style. What none of them has is a reader — and that is not asserted, it is
//! measured: each one has a probe that sets it on a fixture, and none of those probes moves
//! anything the fragment tree or hit testing shows. A row that starts moving something fails.

use crate::parity::support::Support;

/// Why none of these has an effect yet.
const NOTE: &str = "nothing paints yet, so nothing reads it";

crate::register_properties! {
    background_attachment => Support::Ignored(NOTE),
    background_blend_mode => Support::Ignored(NOTE),
    background_clip => Support::Ignored(NOTE),
    background_color => Support::Ignored(NOTE),
    background_image => Support::Ignored(NOTE),
    background_origin => Support::Ignored(NOTE),
    background_position_x => Support::Ignored(NOTE),
    background_position_y => Support::Ignored(NOTE),
    background_repeat => Support::Ignored(NOTE),
    background_size => Support::Ignored(NOTE),
    border_block_end_color => Support::Ignored(NOTE),
    border_block_start_color => Support::Ignored(NOTE),
    border_bottom_color => Support::Ignored(NOTE),
    border_bottom_left_radius => Support::Ignored(NOTE),
    border_bottom_right_radius => Support::Ignored(NOTE),
    border_end_end_radius => Support::Ignored(NOTE),
    border_end_start_radius => Support::Ignored(NOTE),
    border_image_outset => Support::Ignored(NOTE),
    border_image_repeat => Support::Ignored(NOTE),
    border_image_slice => Support::Ignored(NOTE),
    border_image_source => Support::Ignored(NOTE),
    border_image_width => Support::Ignored(NOTE),
    border_inline_end_color => Support::Ignored(NOTE),
    border_inline_start_color => Support::Ignored(NOTE),
    border_left_color => Support::Ignored(NOTE),
    border_right_color => Support::Ignored(NOTE),
    border_start_end_radius => Support::Ignored(NOTE),
    border_top_color => Support::Ignored(NOTE),
    border_top_right_radius => Support::Ignored(NOTE),
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
    outline_color => Support::Ignored(NOTE),
    quotes => Support::Ignored(NOTE),
}
