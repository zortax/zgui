//! Probes for the properties that decide how text is shaped, broken and drawn.
//!
//! Each row names one longhand and a declaration that sets it to something other than its
//! initial value. The declaration is data, not a claim: what it is for is to be applied to a
//! fixture so that the framework can be asked whether anything downstream noticed.

use crate::evidence::probe::Probe;

/// One probe per longhand in this group.
pub static PROBES: &[Probe] = &[
    Probe::new("_webkit_text_security", "-webkit-text-security: disc"),
    Probe::new("caret_color", "caret-color: rgb(3, 5, 7)"),
    Probe::new("color", "color: rgb(3, 5, 7)"),
    Probe::new("color_scheme", "color-scheme: dark"),
    Probe::new("cursor", "cursor: pointer"),
    Probe::new("direction", "direction: rtl"),
    Probe::new("font_family", r#"font-family: "Nonesuch", monospace"#),
    Probe::new(
        "font_feature_settings",
        r#"font-feature-settings: "liga" 0"#,
    ),
    Probe::new("font_kerning", "font-kerning: none"),
    Probe::new("font_language_override", r#"font-language-override: "TRK""#),
    Probe::new("font_optical_sizing", "font-optical-sizing: none"),
    Probe::new("font_size", "font-size: 40px"),
    Probe::new("font_stretch", "font-stretch: 150%"),
    Probe::new("font_style", "font-style: italic"),
    Probe::new("font_synthesis_weight", "font-synthesis-weight: none"),
    Probe::new("font_variant_caps", "font-variant-caps: small-caps"),
    Probe::new("font_variant_east_asian", "font-variant-east-asian: ruby"),
    Probe::new("font_variant_ligatures", "font-variant-ligatures: none"),
    Probe::new("font_variant_numeric", "font-variant-numeric: tabular-nums"),
    Probe::new("font_variant_position", "font-variant-position: sub"),
    Probe::new(
        "font_variation_settings",
        r#"font-variation-settings: "wght" 700"#,
    ),
    Probe::new("font_weight", "font-weight: 900"),
    Probe::new("image_rendering", "image-rendering: pixelated"),
    Probe::new("letter_spacing", "letter-spacing: 4px"),
    Probe::new("line_break", "line-break: strict"),
    Probe::new("line_height", "line-height: 3"),
    Probe::new("overflow_wrap", "overflow-wrap: anywhere"),
    Probe::new("pointer_events", "pointer-events: none"),
    Probe::new("tab_size", "tab-size: 9"),
    Probe::new("text_align", "text-align: right"),
    Probe::new("text_align_last", "text-align-last: right"),
    Probe::new(
        "text_decoration_color",
        "text-decoration-color: rgb(3, 5, 7)",
    ),
    Probe::new("text_decoration_line", "text-decoration-line: underline"),
    Probe::new("text_decoration_style", "text-decoration-style: wavy"),
    Probe::new("text_indent", "text-indent: 13px"),
    Probe::new("text_justify", "text-justify: inter-word"),
    Probe::new("text_overflow", "text-overflow: ellipsis"),
    Probe::new("text_rendering", "text-rendering: optimizeSpeed"),
    Probe::new("text_shadow", "text-shadow: 0 0 9px rgb(1, 2, 3)"),
    Probe::new("text_transform", "text-transform: uppercase"),
    Probe::new("text_wrap_mode", "text-wrap-mode: nowrap"),
    Probe::new("unicode_bidi", "unicode-bidi: bidi-override"),
    Probe::new("visibility", "visibility: collapse"),
    Probe::new("white_space_collapse", "white-space-collapse: preserve"),
    Probe::new("word_break", "word-break: break-all"),
    Probe::new("word_spacing", "word-spacing: 9px"),
    Probe::new("writing_mode", "writing-mode: vertical-rl"),
];
