//! Reading computed values back off the tree.
//!
//! Every case asserts a computed value rather than a call into the matching machinery, because the
//! question is never "does this selector parse" but "does the engine, driven through our traits,
//! arrive at the right answer".

use zgui_dom::{Document, NodeIndex};

/// The computed `color` of one element, as eight-bit sRGB.
///
/// # Panics
///
/// Panics if the element has no computed style.
pub(crate) fn color(document: &Document, index: NodeIndex) -> (u8, u8, u8) {
    let style = document
        .node(index)
        .primary_style()
        .expect("the element is styled");
    let [r, g, b, _] = *style
        .get_inherited_text()
        .clone_color()
        .into_srgb_legacy()
        .raw_components();
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// The computed `border-top-left-radius`, in CSS pixels, or zero when the element has no style.
///
/// A reset property, deliberately: a selector-matching case has to distinguish the elements a rule
/// matched from the elements that merely inherited from one, and an inherited property cannot.
pub(crate) fn radius(document: &Document, index: NodeIndex) -> f32 {
    let Some(style) = document.node(index).primary_style() else {
        return 0.0;
    };
    let radius = style.get_border().clone_border_top_left_radius();
    radius.0.width.0.to_used_value(app_units::Au(0)).to_f32_px()
}

/// The computed `display`.
///
/// # Panics
///
/// Panics if the element has no computed style.
pub(crate) fn display(document: &Document, index: NodeIndex) -> style::values::computed::Display {
    document
        .node(index)
        .primary_style()
        .expect("the element is styled")
        .get_box()
        .clone_display()
}

/// The computed `font-size`, in CSS pixels.
///
/// # Panics
///
/// Panics if the element has no computed style.
pub(crate) fn font_size(document: &Document, index: NodeIndex) -> f32 {
    document
        .node(index)
        .primary_style()
        .expect("the element is styled")
        .get_font()
        .clone_font_size()
        .computed_size()
        .px()
}
