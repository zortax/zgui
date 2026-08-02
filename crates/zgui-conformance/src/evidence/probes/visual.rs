//! Probes for the properties that decide what a box looks like.
//!
//! Each row names one longhand and a declaration that sets it to something other than its
//! initial value. The declaration is data, not a claim: what it is for is to be applied to a
//! fixture so that the framework can be asked whether anything downstream noticed.

use crate::evidence::probe::Probe;

/// One probe per longhand in this group.
pub static PROBES: &[Probe] = &[
    Probe::new("backdrop_filter", "backdrop-filter: blur(4px)"),
    Probe::new("background_attachment", "background-attachment: fixed"),
    Probe::new("background_blend_mode", "background-blend-mode: multiply"),
    Probe::new("background_clip", "background-clip: content-box"),
    Probe::new("background_color", "background-color: rgb(3, 5, 7)"),
    Probe::new(
        "background_image",
        "background-image: linear-gradient(red, blue)",
    ),
    Probe::new("background_origin", "background-origin: content-box"),
    Probe::new("background_position_x", "background-position-x: 7px"),
    Probe::new("background_position_y", "background-position-y: 9px"),
    Probe::new("background_repeat", "background-repeat: no-repeat"),
    Probe::new("background_size", "background-size: 11px 13px"),
    Probe::new(
        "border_block_end_color",
        "border-block-end-color: rgb(3, 5, 7)",
    ),
    Probe::new("border_block_end_style", "border-block-end-style: dashed"),
    Probe::in_context(
        "border_block_end_width",
        "border-block-end-style: solid",
        "border-block-end-width: 7px",
    ),
    Probe::new(
        "border_block_start_color",
        "border-block-start-color: rgb(3, 5, 7)",
    ),
    Probe::new(
        "border_block_start_style",
        "border-block-start-style: dashed",
    ),
    Probe::in_context(
        "border_block_start_width",
        "border-block-start-style: solid",
        "border-block-start-width: 7px",
    ),
    Probe::new("border_bottom_color", "border-bottom-color: rgb(3, 5, 7)"),
    Probe::new(
        "border_bottom_left_radius",
        "border-bottom-left-radius: 9px",
    ),
    Probe::new(
        "border_bottom_right_radius",
        "border-bottom-right-radius: 9px",
    ),
    Probe::new("border_bottom_style", "border-bottom-style: dashed"),
    Probe::in_context(
        "border_bottom_width",
        "border-bottom-style: solid",
        "border-bottom-width: 7px",
    ),
    Probe::new("border_end_end_radius", "border-end-end-radius: 9px"),
    Probe::new("border_end_start_radius", "border-end-start-radius: 9px"),
    Probe::new("border_image_outset", "border-image-outset: 3px"),
    Probe::new("border_image_repeat", "border-image-repeat: round"),
    Probe::new("border_image_slice", "border-image-slice: 20%"),
    Probe::new(
        "border_image_source",
        "border-image-source: linear-gradient(red, blue)",
    ),
    Probe::new("border_image_width", "border-image-width: 7px"),
    Probe::new(
        "border_inline_end_color",
        "border-inline-end-color: rgb(3, 5, 7)",
    ),
    Probe::new("border_inline_end_style", "border-inline-end-style: dashed"),
    Probe::in_context(
        "border_inline_end_width",
        "border-inline-end-style: solid",
        "border-inline-end-width: 7px",
    ),
    Probe::new(
        "border_inline_start_color",
        "border-inline-start-color: rgb(3, 5, 7)",
    ),
    Probe::new(
        "border_inline_start_style",
        "border-inline-start-style: dashed",
    ),
    Probe::in_context(
        "border_inline_start_width",
        "border-inline-start-style: solid",
        "border-inline-start-width: 7px",
    ),
    Probe::new("border_left_color", "border-left-color: rgb(3, 5, 7)"),
    Probe::new("border_left_style", "border-left-style: dashed"),
    Probe::in_context(
        "border_left_width",
        "border-left-style: solid",
        "border-left-width: 7px",
    ),
    Probe::new("border_right_color", "border-right-color: rgb(3, 5, 7)"),
    Probe::new("border_right_style", "border-right-style: dashed"),
    Probe::in_context(
        "border_right_width",
        "border-right-style: solid",
        "border-right-width: 7px",
    ),
    Probe::new("border_start_end_radius", "border-start-end-radius: 9px"),
    Probe::new(
        "border_start_start_radius",
        "border-start-start-radius: 9px",
    ),
    Probe::new("border_top_color", "border-top-color: rgb(3, 5, 7)"),
    Probe::new("border_top_left_radius", "border-top-left-radius: 9px"),
    Probe::new("border_top_right_radius", "border-top-right-radius: 9px"),
    Probe::new("border_top_style", "border-top-style: dashed"),
    Probe::in_context(
        "border_top_width",
        "border-top-style: solid",
        "border-top-width: 7px",
    ),
    Probe::new("box_shadow", "box-shadow: 0 0 9px rgb(1, 2, 3)"),
    Probe::new("clip", "clip: rect(1px, 2px, 3px, 4px)"),
    Probe::new("clip_path", "clip-path: inset(3px)"),
    Probe::new("content", r#"content: "x""#),
    Probe::new(
        "corner_bottom_left_shape",
        "corner-bottom-left-shape: squircle",
    ),
    Probe::new(
        "corner_bottom_right_shape",
        "corner-bottom-right-shape: squircle",
    ),
    Probe::new("corner_end_end_shape", "corner-end-end-shape: squircle"),
    Probe::new("corner_end_start_shape", "corner-end-start-shape: squircle"),
    Probe::new("corner_start_end_shape", "corner-start-end-shape: squircle"),
    Probe::new(
        "corner_start_start_shape",
        "corner-start-start-shape: squircle",
    ),
    Probe::new("corner_top_left_shape", "corner-top-left-shape: squircle"),
    Probe::new("corner_top_right_shape", "corner-top-right-shape: squircle"),
    Probe::new("counter_increment", "counter-increment: chapter 2"),
    Probe::new("counter_reset", "counter-reset: chapter 2"),
    Probe::new("filter", "filter: blur(4px)"),
    Probe::new(
        "list_style_image",
        "list-style-image: linear-gradient(red, blue)",
    ),
    Probe::new("list_style_position", "list-style-position: inside"),
    Probe::new("list_style_type", "list-style-type: square"),
    Probe::new("mask_clip", "mask-clip: content-box"),
    Probe::new("mask_composite", "mask-composite: subtract"),
    Probe::new("mask_image", "mask-image: linear-gradient(red, blue)"),
    Probe::new("mask_mode", "mask-mode: luminance"),
    Probe::new("mask_origin", "mask-origin: content-box"),
    Probe::new("mask_position_x", "mask-position-x: 7px"),
    Probe::new("mask_position_y", "mask-position-y: 9px"),
    Probe::new("mask_repeat", "mask-repeat: no-repeat"),
    Probe::new("mask_size", "mask-size: 11px 13px"),
    Probe::new("mask_type", "mask-type: alpha"),
    Probe::new("mix_blend_mode", "mix-blend-mode: multiply"),
    Probe::new("opacity", "opacity: 0.5"),
    Probe::new("outline_color", "outline-color: rgb(3, 5, 7)"),
    Probe::in_context(
        "outline_offset",
        "outline-style: solid",
        "outline-offset: 6px",
    ),
    Probe::new("outline_style", "outline-style: dashed"),
    Probe::in_context(
        "outline_width",
        "outline-style: solid",
        "outline-width: 7px",
    ),
    Probe::new("quotes", r#"quotes: "<" ">""#),
];
