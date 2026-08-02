//! The colour of a run, kept apart from everything else about it.

use zgui_color::Color;
use zgui_css::PinnedGroup;
use zgui_css::computed::style_structs;

/// The identity a brush slot is claimed against.
///
/// A handle rather than a number, and that is the whole of the type's purpose. Slots are claimed by
/// *cascade result*, so the identity is the address of the group the colour was cascaded into — and
/// an address is only an identity while the allocation behind it is alive. A style whose last
/// reference is dropped frees its groups, the next style built lands on the same addresses, and a
/// table holding bare numbers hands the new style's colour to every shaped paragraph that claimed a
/// slot against the old one. Holding the handle is what stops the address coming back.
///
/// The cost is that a table keyed on these pins one property group per slot, so a long-lived table
/// has to release the slots it stops using. That is the deliberate trade: the alternative is a
/// table that is silently wrong.
pub type TextPaintKey = PinnedGroup<style_structs::InheritedText>;

/// The colour a run is drawn in, together with the identity of the cascade result it came from.
///
/// The two halves are for two different consumers. The colour is what a brush table entry holds;
/// the key is what claims the slot. Slots are claimed by *cascade result*, not by resolved colour,
/// which is what keeps a paragraph whose colour came from a theme token separate from one whose
/// colour was written literally — otherwise re-theming the first would silently re-colour the
/// second.
///
/// Nothing here is part of a shaping or breaking key, and that is the whole point: changing a
/// colour must cost a table write, never a shape.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPaint {
    /// The identity of the cascade result the colour came from.
    pub key: TextPaintKey,
    /// The resolved colour.
    pub color: Color,
}
