//! What the fragment pass turns into stacking contexts, transforms, clips, ink extents and hit
//! answers.
//!
//! Every row here is evidence-backed: setting the property on a fixture changes the fragment
//! tree or the answer hit testing gives, and a row whose probe stops showing that fails.

use zgui_css::parity::Support;

/// Where these properties are read.
const READER: &str = "zgui-layout::fragment";

zgui_css::register_properties! {
    backdrop_filter => Support::Implemented(READER),
    border_start_start_radius => Support::Implemented(READER),
    border_top_left_radius => Support::Implemented(READER),
    box_shadow => Support::Implemented(READER),
    clip_path => Support::Implemented(READER),
    filter => Support::Implemented(READER),
    isolation => Support::Implemented(READER),
    mix_blend_mode => Support::Implemented(READER),
    opacity => Support::Implemented(READER),
    outline_offset => Support::Implemented(READER),
    outline_style => Support::Implemented(READER),
    outline_width => Support::Implemented(READER),
    pointer_events => Support::Implemented(READER),
    rotate => Support::Implemented(READER),
    scale => Support::Implemented(READER),
    transform => Support::Implemented(READER),
    transform_origin => Support::Implemented(READER),
    translate => Support::Implemented(READER),
    z_index => Support::Implemented(READER),
}
