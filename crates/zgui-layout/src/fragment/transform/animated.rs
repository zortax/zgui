//! The transform an animation is imposing on a box, in place of the one its own style asks for.
//!
//! An animation that moves only where a box is drawn does not go through the cascade: it writes the
//! interpolated placement into a table of its own, and this pass composes against that table
//! instead of against the box's shared style for that one question. Every other question the style
//! answers — what establishes a stacking context, what is a containing block, what the box is
//! painted in — is answered from the style, unchanged, because a placement is only ever admitted
//! for an element whose answers to those questions the animation does not move.
//!
//! The table is a sorted slice rather than a map because it holds one entry per *animating*
//! element, which is a handful on a screen and never a document: a binary search over eight entries
//! is cheaper than a hash, and building a map per frame would cost more than every lookup in it.

use zgui_dom::NodeKey;
use zgui_dom::side::AnimPlacement;

/// The placement `node` is being drawn under, if an animation is moving it.
///
/// `placements` is sorted by element. An anonymous box has no element and therefore no placement:
/// it is carried by the matrix its generator composed, exactly as every other descendant is.
pub fn of(
    placements: &[(NodeKey, AnimPlacement)],
    node: Option<NodeKey>,
) -> Option<&AnimPlacement> {
    let node = node?;
    placements
        .binary_search_by_key(&node, |(held, _)| *held)
        .ok()
        .map(|position| &placements[position].1)
}

#[cfg(test)]
mod tests {
    use super::of;

    #[test]
    fn a_box_with_no_element_is_never_placed_by_one() {
        assert!(of(&[], None).is_none());
    }
}
