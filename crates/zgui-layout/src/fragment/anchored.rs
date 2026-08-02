//! Which boxes a scroll carries with it, and which stay where they are.
//!
//! A scroll is composed rather than laid out: nothing about the boxes inside a scroll container
//! changes when it is scrolled, so the offset is subtracted once, at the container, and carried
//! down as a shift that every descendant's origin is written with. That is right for every box
//! whose containing block is inside the container — which is nearly all of them.
//!
//! It is wrong for exactly one kind. A `position: fixed` box's containing block is the viewport,
//! not any ancestor, so scrolling an ancestor must not move it: that is the whole of what `fixed`
//! means, and it is what a floating surface, a masthead pinned to the top of the window and a
//! modal's scrim are all built out of. A shift applied to one of those carries it off the screen at
//! exactly the rate the page is scrolled, and every measurement taken inside the process still
//! agrees with itself — the box has a box, it has the size it asked for, it is in the display list.
//! It is simply somewhere nobody can see.

use zgui_css::ComputedStyle;
use zgui_css::values::size::PositionValue;

/// Whether `style` puts the box in the viewport rather than in whatever is scrolling around it.
///
/// The one question a caller composing an origin has to ask: a box that answers `true` takes no
/// part of the accumulated scroll shift, and neither does anything inside it.
pub(crate) fn ignores_scroll(style: &ComputedStyle) -> bool {
    style.get_box().position == PositionValue::Fixed
}
