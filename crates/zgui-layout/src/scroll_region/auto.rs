//! The `overflow: auto` fixpoint.
//!
//! `auto` is the one overflow value whose effect on layout depends on the result of that layout:
//! it reserves a scrollbar gutter exactly when the content overflows, and whether the content
//! overflows depends on how much room the gutter left it. The layout algorithms have no such value,
//! so it enters as `hidden` — reserving nothing — and a box that turns out to overflow is laid out
//! again with the gutter reserved.
//!
//! Two iterations, which is what browsers do. Reserving a gutter can only make the content narrower
//! and therefore taller, so a third pass changes the answer only in a configuration that oscillates,
//! and an oscillating box is better left one pixel wrong than laid out forever.

use zgui_dom::side::BoxKey;

use crate::measure::MeasureContent;
use crate::style::convert::overflow::is_undecided;
use crate::tree::LayoutTree;

/// How many times the decision may be revised before it is taken as final.
pub const MAX_PASSES: u32 = 2;

/// Revises every undecided box's gutter against the layout that has just been computed.
///
/// Returns whether any decision changed, which is what tells the caller another layout pass is
/// owed. The decisions themselves are kept on the boxes, so the next frame starts from the previous
/// answer rather than from `hidden` — a scrollport that was scrolling last frame does not flicker
/// its gutter on every keystroke inside it.
pub fn revise<C: MeasureContent>(tree: &mut LayoutTree<'_, C>, root: BoxKey) -> bool {
    let mut changed = false;
    for key in tree.store().keys() {
        let Some(node) = tree.store().get(key) else {
            continue;
        };
        let box_ = node.style.get_box();
        let horizontal = is_undecided(box_.overflow_x);
        let vertical = is_undecided(box_.overflow_y);
        if !horizontal && !vertical {
            continue;
        }
        let Some(layout) = tree.store().layout_of(key) else {
            continue;
        };
        let held = tree.store().auto_scroll(key);
        let wanted = (
            horizontal && layout.content_size.width.0 > layout.size.width.0 + EPSILON,
            vertical && layout.content_size.height.0 > layout.size.height.0 + EPSILON,
        );
        if wanted == held {
            continue;
        }
        tree.store_mut().set_auto_scroll(key, wanted);
        crate::tree::dirty::mark_dirty(tree.store_mut(), key);
        changed = true;
    }
    if changed {
        crate::tree::dirty::mark_dirty(tree.store_mut(), root);
    }
    changed
}

/// How much a content size may exceed a box before it counts as overflowing.
///
/// Both numbers come out of the same accumulation of floating-point additions, so a box whose
/// content exactly fills it can report a content size a fraction of a pixel larger. Reserving a
/// scrollbar for that is a visible bug — and one that oscillates, because reserving the gutter then
/// makes the content genuinely overflow.
const EPSILON: f32 = 1.0 / 64.0;

#[cfg(test)]
mod tests {
    use super::{EPSILON, MAX_PASSES};

    #[test]
    fn the_tolerance_is_below_a_device_pixel_and_the_cap_is_two() {
        const { assert!(EPSILON > 0.0 && EPSILON < 1.0) };
        assert_eq!(MAX_PASSES, 2);
    }
}
