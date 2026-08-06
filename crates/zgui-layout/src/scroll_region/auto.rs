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
use zgui_profile::{Counter, counter};

use crate::measure::MeasureContent;
use crate::tree::LayoutTree;

/// How many times the decision may be revised before it is taken as final.
pub const MAX_PASSES: u32 = 2;

/// Revises every undecided box's gutter against the layout that has just been computed.
///
/// Returns whether any decision changed, which is what tells the caller another layout pass is
/// owed. The decisions themselves are kept on the boxes, so the next frame starts from the previous
/// answer rather than from `hidden` — a scrollport that was scrolling last frame does not flicker
/// its gutter on every keystroke inside it.
///
/// # Only the boxes that can change an answer
///
/// The undecided boxes are the ones written `overflow: auto`, and every other box in the document
/// is one this would look at and immediately skip. So it walks the roster of them rather than the
/// tree — see [`roster`](crate::tree::store::roster) — which makes this cost what the document
/// *scrolls* rather than what it contains. A document with no `auto` anywhere, which is most of
/// them, does not run at all.
///
/// The entries are compacted as they are read. A box whose overflow was restyled away is dropped
/// here rather than at the restyle, and so is one that is no longer live: the roster holds every
/// box ever registered, while the walk this replaced started at the root and so saw only boxes
/// still attached to the tree. Reviving a detached box's gutter would mark a chain that never
/// reaches the root, and buy a second layout pass that changes nothing.
pub fn revise<C: MeasureContent>(tree: &mut LayoutTree<'_, C>, root: BoxKey) -> bool {
    if tree.store().no_undecided_overflow() {
        return false;
    }
    let mut changed = false;
    let mut roster = tree.store_mut().take_overflow_roster();
    roster.entries.retain(|&key| {
        if !tree.store().contains(key) {
            return false;
        }
        let (horizontal, vertical) = tree.store().undecided_overflow(key);
        if !horizontal && !vertical {
            return false;
        }
        counter::bump(Counter::GuttersExamined);
        let Some(layout) = tree.store().layout_of(key) else {
            return true;
        };
        let held = tree.store().auto_scroll(key);
        let wanted = (
            horizontal && layout.content_size.width.0 > layout.size.width.0 + EPSILON,
            vertical && layout.content_size.height.0 > layout.size.height.0 + EPSILON,
        );
        if wanted == held {
            return true;
        }
        tree.store_mut().set_auto_scroll(key, wanted);
        crate::tree::dirty::mark_dirty(tree.store_mut(), key);
        changed = true;
        true
    });
    tree.store_mut().restore_overflow_roster(roster);
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
