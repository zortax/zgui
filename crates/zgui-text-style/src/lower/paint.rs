//! The colour, and the key a brush slot is claimed against.

use zgui_css::values::color::to_color;
use zgui_css::{ComputedStyle, PinnedGroup};

use crate::style::paint::TextPaint;

/// The colour a run is drawn in, and the identity of the cascade result it came from.
///
/// The key is the identity of the group the colour was cascaded into, not a hash of the colour.
/// Two runs share a brush slot exactly when they share that group, which is the set a theme change
/// re-resolves together — so re-theming rewrites one entry per group and every shaped paragraph
/// pointing at it changes colour without being touched.
///
/// Hashing the colour instead would put a run whose colour came from a theme token in the same slot
/// as one whose colour was written literally, and re-colouring the first would silently re-colour
/// the second.
///
/// The identity is a handle rather than the address as a number, and the difference is not
/// cosmetic: a number outlives the allocation it names, and a table holding one answers the next
/// style that lands on the address with the previous style's slot.
pub fn paint(style: &ComputedStyle) -> TextPaint {
    TextPaint {
        key: PinnedGroup::inherited_text(style),
        color: to_color(&style.get_inherited_text().color),
    }
}
