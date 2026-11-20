//! Whether a style change leaves every fragment of a box as it is.
//!
//! A fragment is made of layout results and style: ink, flags, filters, clip radii, read extents and
//! blending all come from groups the change can reach. A pass that keeps a fragment needs a proof
//! that the fragment would come out the same, and the only place that holds the old and the new
//! style together is the write of the new one.
//!
//! The proof is deliberately narrow. Every group must be equal apart from the text, border and
//! outline colours and the background colour and images, which only the painter reads. A background
//! also decides [`FragmentFlags::PAINTS_NOTHING`](crate::fragment::FragmentFlags::PAINTS_NOTHING),
//! so a background that starts or stops painting gives no proof.
//!
//! Three kinds of box get no proof from a colour either, and the caller refuses them:
//!
//! - A box that declares a framework custom property. A filter or a shape reads its parameters
//!   from the custom property map, and a theme change moves that map.
//! - A nested inline element. The lines it sits on carry its painting in their content hash.
//! - A box a mark reached with no new style. An animation or a text edit marks a repaint too.

use zgui_css::ComputedStyle;

use crate::fragment::ink;

/// Whether `old` and `new` differ in nothing but the colours the painter alone reads.
pub(crate) fn only_colours_differ(old: &ComputedStyle, new: &ComputedStyle) -> bool {
    macro_rules! same {
        ($get:ident) => {
            core::ptr::eq(old.$get(), new.$get()) || old.$get() == new.$get()
        };
    }
    old.flags == new.flags
        && old.writing_mode == new.writing_mode
        && old.effective_zoom == new.effective_zoom
        && same!(get_box)
        && same!(get_column)
        && same!(get_counters)
        && same!(get_effects)
        && same!(get_font)
        && same!(get_inherited_box)
        && same!(get_inherited_table)
        && same!(get_inherited_ui)
        && same!(get_list)
        && same!(get_margin)
        && same!(get_padding)
        && same!(get_position)
        && same!(get_svg)
        && same!(get_table)
        && same!(get_text)
        && same!(get_ui)
        && same_outline(old, new)
        && same_text(old, new)
        && same_border(old, new)
        && same_background(old, new)
}

/// Whether the outline groups are equal apart from the colour.
fn same_outline(old: &ComputedStyle, new: &ComputedStyle) -> bool {
    let (old, new) = (old.get_outline(), new.get_outline());
    if core::ptr::eq(old, new) {
        return true;
    }
    let mut masked = new.clone();
    masked.outline_color = old.outline_color.clone();
    masked == *old
}

/// Whether the text groups are equal apart from `color`.
fn same_text(old: &ComputedStyle, new: &ComputedStyle) -> bool {
    let (old, new) = (old.get_inherited_text(), new.get_inherited_text());
    if core::ptr::eq(old, new) {
        return true;
    }
    let mut masked = new.clone();
    masked.color = old.color;
    masked == *old
}

/// Whether the border groups are equal apart from the four colours.
fn same_border(old: &ComputedStyle, new: &ComputedStyle) -> bool {
    let (old, new) = (old.get_border(), new.get_border());
    if core::ptr::eq(old, new) {
        return true;
    }
    let mut masked = new.clone();
    masked.border_top_color = old.border_top_color.clone();
    masked.border_right_color = old.border_right_color.clone();
    masked.border_bottom_color = old.border_bottom_color.clone();
    masked.border_left_color = old.border_left_color.clone();
    masked == *old
}

/// Whether the background groups are equal apart from what is painted.
///
/// The colour may stay visible or stay clear, and the images may change while some layer stays
/// set. Layout never reads them, and ink asks only whether they paint.
fn same_background(old: &ComputedStyle, new: &ComputedStyle) -> bool {
    if ink::background_colour_is_clear(old) != ink::background_colour_is_clear(new)
        || ink::no_background_image(old) != ink::no_background_image(new)
    {
        return false;
    }
    let (old, new) = (old.get_background(), new.get_background());
    if core::ptr::eq(old, new) {
        return true;
    }
    let mut masked = new.clone();
    masked.background_color = old.background_color.clone();
    masked.background_image = old.background_image.clone();
    masked == *old
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_css::values::color::AbsoluteColor;

    use super::only_colours_differ;

    /// A colour.
    fn colour(red: u8) -> AbsoluteColor {
        AbsoluteColor::srgb_legacy(red, 0, 0, 1.0)
    }

    #[test]
    fn a_new_text_colour_is_only_a_colour() {
        let old = StyleDraft::initial().build();
        let mut draft = StyleDraft::from_style(&old);
        draft.inherited_text().color = colour(9);
        assert!(only_colours_differ(&old, &draft.build()));
    }

    #[test]
    fn a_new_font_size_is_more_than_a_colour() {
        let old = StyleDraft::initial().build();
        let new = StyleDraft::from_style(&old)
            .with_font_size(zgui_geom::CssPx(23.0))
            .build();
        assert!(!only_colours_differ(&old, &new));
    }
}
