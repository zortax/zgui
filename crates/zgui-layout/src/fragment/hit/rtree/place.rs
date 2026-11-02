//! Telling the tree where an entry has moved to, in the three ways that can be answered.

use zgui_geom::{Device, DevicePx, Rect};

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::home::Homes;
use crate::fragment::hit::rtree::{RTree, envelope};

/// Which of the three answers a placement took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placed {
    /// The rectangle still lay inside the leaf holding it, so nothing but the entry was touched.
    InPlace,
    /// It had left that leaf, but stayed inside the branch above it or only grew where it was, so
    /// that leaf's rectangle was widened and the envelopes above it were recomputed.
    Stretched,
    /// It had gone somewhere else entirely, so it was taken out and put back.
    Reinserted,
}

/// Records that one entry now covers `bounds`.
///
/// # Why a fragment that moved usually keeps its leaf
///
/// The hierarchy exists to dismiss subtrees, and it is good at that when the entries under one node
/// are near each other. A scroll moves every entry under the scroller by the same vector, so a leaf
/// whose entries were neighbours before is a leaf whose entries are neighbours after — the grouping
/// a re-insertion would search for is the grouping the tree already has. Taking each entry out and
/// putting it back would rediscover it, one descent and one `choose` per level per entry, and
/// arrive where it started.
///
/// So an entry that has not left the region its own branch of the hierarchy covers is rewritten
/// where it lies. The branch is the test rather than the leaf, because a leaf's rectangle is drawn
/// tightly around eight neighbours and a list scrolling at speed moves each of them past the next
/// one within a frame — while the node above holds the whole neighbourhood those eight were grouped
/// out of, and a rectangle still inside it has not left the region the descent already routes here.
/// Widening the leaf to take it in therefore changes nothing above the leaf at all: its parent
/// already contains the new rectangle, so the climb that follows stops one level up. It also bounds
/// how loose a leaf can become: only an entry that grew (below) takes it past the node it hangs
/// from.
///
/// An entry that grew where it was keeps its leaf too, wherever its edges went. A resize widens
/// every row of a page by the same amount in one frame, and the first row of each branch to widen
/// leaves that branch. A search moves it into whichever leaf widened first, which groups the
/// entries by the order of the writes, and every search counts towards a rebuild of the whole
/// index. Its old rectangle is inside the new one, so the leaf grows by what the entry grew and
/// keeps the neighbours it had.
///
/// An entry whose new rectangle has left that branch, and has not just grown, has genuinely gone
/// somewhere else, and for that one the search is the right answer: keeping it would stretch its
/// leaf across the gap and make the node answer for a region nothing in it occupies.
pub(crate) fn at(
    tree: &mut RTree,
    key: FragKey,
    bounds: Rect<DevicePx, Device>,
    homes: &mut Homes,
) -> Placed {
    let Some(leaf) = tree.leaf_of(key, homes) else {
        tree.insert(key, bounds, homes);
        return Placed::Reinserted;
    };
    let Some(slot) = tree.nodes[leaf].slot_of(key) else {
        tree.remove(key, homes);
        tree.insert(key, bounds, homes);
        return Placed::Reinserted;
    };
    if tree.nodes[leaf].envelope.contains_rect(bounds) {
        tree.nodes[leaf].entries[slot].1 = bounds;
        return Placed::InPlace;
    }
    if branch_holds(tree, leaf, bounds) || grew(tree.nodes[leaf].entries[slot].1, bounds) {
        tree.nodes[leaf].entries[slot].1 = bounds;
        envelope::refresh(tree, leaf);
        return Placed::Stretched;
    }
    tree.remove(key, homes);
    tree.insert(key, bounds, homes);
    Placed::Reinserted
}

/// Whether the node one leaf hangs from already covers `bounds`.
///
/// A leaf that is the whole tree hangs from nothing, and there is no branch to stay inside: every
/// rectangle it is given is inside it by definition, because its envelope is the union of what it
/// holds and it is about to be recomputed.
fn branch_holds(tree: &RTree, leaf: usize, bounds: Rect<DevicePx, Device>) -> bool {
    match tree.nodes[leaf].parent {
        Some(parent) => tree.nodes[parent].envelope.contains_rect(bounds),
        None => true,
    }
}

/// Whether `bounds` holds all of `held`, a rectangle with an area: the entry grew where it was.
///
/// An empty rectangle has no place to grow from, so an entry that was empty is not said to grow.
fn grew(held: Rect<DevicePx, Device>, bounds: Rect<DevicePx, Device>) -> bool {
    !held.is_empty() && bounds.contains_rect(held)
}
