//! The style of a box that no element wrote, built so that it shares what it inherited.
//!
//! A box CSS requires but no element declares — the anonymous box wrapping a run of inline
//! children, and the run of text inside it — is styled with the inherited properties of the element
//! it hangs under and the initial value of everything else.
//!
//! What makes that worth its own module is *sharing*. A computed style holds one shared pointer per
//! property group, and everything downstream keys its work on those pointers: which font a run is
//! shaped with, which brush slot its glyphs are drawn through, whether a cached lowering may be
//! reused. A style that copies the parent's inherited values into fresh allocations agrees with the
//! parent on every property and is a stranger to all of them — so the run's glyphs claim a brush
//! slot no element owns, and a colour written through the element's slot re-colours nothing that is
//! on the screen.

use std::sync::OnceLock;
use style::custom_properties::ComputedCustomProperties;
use style::properties::{ComputedValues, ComputedValuesInner, style_structs};

use crate::computed::style::ComputedStyle;

/// A style that inherits from `parent` and holds the initial value of everything else.
///
/// The inherited groups are the parent's own allocations rather than copies of them, which is what
/// makes the box agree with its element about the things identity is read for: the face its text is
/// shaped with, and the cascade result its glyphs claim a brush slot against. The reset groups are
/// shared too — every box built here holds the same initial background, border and padding — so a
/// document of a thousand paragraphs allocates none of them a thousand times.
///
/// It is deliberately not a copy of the parent's style: a box that inherited its parent's borders,
/// background and padding would paint all three a second time.
///
/// ```
/// use zgui_css::{PinnedGroup, StyleDraft, inherited_style};
///
/// let parent = StyleDraft::initial().build();
/// let anonymous = inherited_style(&parent);
///
/// assert_eq!(
///     PinnedGroup::inherited_text(&anonymous),
///     PinnedGroup::inherited_text(&parent),
///     "the box inherited the colour, so it inherited the identity the colour is keyed by",
/// );
/// assert_eq!(anonymous.get_padding(), parent.get_padding(), "both are initial");
/// ```
pub fn inherited_style(parent: &ComputedStyle) -> ComputedStyle {
    // Both are read as the record of groups rather than as a style, because a style also answers
    // every longhand by name and several of those names are a group's name as well.
    let reset: &ComputedValuesInner = reset_values();
    // Custom properties inherit, and the ones that do not are the element's own.
    let inherited_custom = parent.custom_properties().inherited.clone();
    let parent: &ComputedValuesInner = parent;
    ComputedValues::new(
        None,
        ComputedCustomProperties {
            inherited: inherited_custom,
            non_inherited: Default::default(),
        },
        Default::default(),
        parent.writing_mode,
        parent.effective_zoom,
        parent.flags.inherited(),
        None,
        None,
        reset.clone_background(),
        reset.clone_border(),
        reset.clone_box(),
        reset.clone_column(),
        reset.clone_counters(),
        reset.clone_effects(),
        parent.clone_font(),
        parent.clone_inherited_box(),
        parent.clone_inherited_table(),
        parent.clone_inherited_text(),
        parent.clone_inherited_ui(),
        parent.clone_list(),
        reset.clone_margin(),
        reset.clone_outline(),
        reset.clone_padding(),
        reset.clone_position(),
        reset.clone_svg(),
        reset.clone_table(),
        reset.clone_text(),
        reset.clone_ui(),
    )
}

/// The style every reset group is taken from, built once for the life of the process.
///
/// Built once because the groups are handed out by pointer: a fresh set per box would be a fresh
/// identity per box, and every consumer that keys work on a group would do that work once per
/// anonymous box instead of once.
fn reset_values() -> &'static ComputedStyle {
    static RESET: OnceLock<ComputedStyle> = OnceLock::new();
    RESET.get_or_init(|| {
        let mut font = style_structs::Font::initial_values();
        font.compute_font_hash();
        ComputedValues::initial_values_with_font_override(font)
    })
}

#[cfg(test)]
mod tests {
    use zgui_geom::CssPx;

    use super::inherited_style;
    use crate::computed::draft::StyleDraft;
    use crate::computed::pinned::PinnedGroup;
    use crate::computed::style::StructPtr;

    /// A parent whose text colour and font size are its own.
    fn parent() -> crate::computed::style::ComputedStyle {
        let mut draft = StyleDraft::initial().with_font_size(CssPx(23.0));
        draft.inherited_text().color =
            crate::values::color::AbsoluteColor::srgb_legacy(64, 128, 192, 1.0);
        draft.build()
    }

    /// The property this module exists for: the identity, not merely the value, is inherited.
    ///
    /// A brush slot is claimed against the identity of the cascade result a run inherited its
    /// colour from. Copying the values into a new allocation gives the run an identity no element
    /// has, so the colour an element's slot is rewritten with reaches none of its text.
    #[test]
    fn the_inherited_groups_are_the_parents_own_allocations() {
        let parent = parent();
        let anonymous = inherited_style(&parent);

        assert_eq!(
            PinnedGroup::inherited_text(&anonymous),
            PinnedGroup::inherited_text(&parent),
            "the run's colour is the element's, so it has to be keyed by the element's identity"
        );
        assert_eq!(
            StructPtr::of(anonymous.get_font()),
            StructPtr::of(parent.get_font()),
            "a run that inherits the face has to be shaped by the same cached lowering as its \
             element, which is keyed on this pointer"
        );
        assert_eq!(
            StructPtr::of(anonymous.get_inherited_box()),
            StructPtr::of(parent.get_inherited_box())
        );
        assert_eq!(
            StructPtr::of(anonymous.get_list()),
            StructPtr::of(parent.get_list())
        );
    }

    /// The other half: an anonymous box must not paint its element's decorations a second time.
    #[test]
    fn nothing_that_does_not_inherit_is_taken_from_the_parent() {
        let mut draft = StyleDraft::from_style(&parent());
        draft.box_group().display = crate::values::size::DisplayValue::Block;
        let padded = draft.build();
        let anonymous = inherited_style(&padded);
        let initial = StyleDraft::initial().build();

        assert_eq!(
            anonymous.get_padding(),
            initial.get_padding(),
            "the wrapper took its element's padding, so the element's box is inset twice"
        );
        assert_eq!(anonymous.get_background(), initial.get_background());
        assert_eq!(anonymous.get_border(), initial.get_border());
        assert_eq!(
            anonymous.clone_display(),
            initial.clone_display(),
            "display does not inherit, and a wrapper that took it lays its run out by the \
             element's rules rather than as the inline context it was created to establish"
        );
    }

    /// Two boxes built here share their reset groups, which is what keeps a long document cheap.
    #[test]
    fn every_synthesised_style_shares_one_set_of_reset_groups() {
        let one = inherited_style(&parent());
        let two = inherited_style(&StyleDraft::initial().build());

        assert_eq!(
            StructPtr::of(one.get_padding()),
            StructPtr::of(two.get_padding()),
            "each anonymous box allocated its own initial padding"
        );
        assert_ne!(
            PinnedGroup::inherited_text(&one),
            PinnedGroup::inherited_text(&two),
            "two boxes under different elements must not share what they inherited"
        );
    }

    /// The values are the parent's, not just the pointers.
    #[test]
    fn the_inherited_values_are_the_parents() {
        let parent = parent();
        let anonymous = inherited_style(&parent);

        assert_eq!(
            anonymous.get_inherited_text().color,
            parent.get_inherited_text().color
        );
        assert_eq!(
            anonymous.get_font().font_size.used_size().px(),
            23.0,
            "a run shaped at the initial size rather than its element's is the wrong size"
        );
    }
}
