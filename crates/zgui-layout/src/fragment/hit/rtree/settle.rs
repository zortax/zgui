//! Taking a whole subtree's worth of moved rectangles in one pass.
//!
//! # Why this is not the same call repeated
//!
//! [`place::at`](super::place::at) answers one entry at a time, and the answer it can give depends
//! on where the *other* entries in the same leaf still are. A scroll moves every entry under the
//! scroller by the same vector, so answering them one at a time asks each of them to fit inside a
//! leaf still drawn around its neighbours' old positions — and each one that does not fit stretches
//! its leaf across the gap, or, once the gap outgrows the branch above, is taken out of the
//! hierarchy and searched back into it. That is a descent and a choice per level, thousands of times
//! a frame, to rediscover the grouping the tree already had: the entries that were neighbours before
//! the scroll are the entries that are neighbours after it.
//!
//! Taken together the question does not arise. Every rectangle is written where its entry already
//! lies, no entry changes leaf, and each envelope is then recomputed once as the union of what is
//! actually below it. A leaf whose entries all moved by the same vector comes out exactly one vector
//! further along — tighter than any sequence of stretches would have left it.
//!
//! # What the hierarchy is allowed to be
//!
//! One thing here is a correctness claim: **every node's envelope contains every rectangle below
//! it**. A query descends by dismissing nodes whose envelope misses the point and then tests each
//! surviving entry's own rectangle, so which leaf an entry sits in decides what a query costs and
//! never what it answers. Recomputing every touched envelope as an exact union is what establishes
//! that claim, and it establishes it whether or not the grouping it was built on was a good one.

use zgui_geom::{Device, DevicePx, Rect};
use zgui_profile::{Counter, counter};

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::home::Homes;
use crate::fragment::hit::rtree::{Carried, RTree, envelope_of};

/// Writes every rectangle in `moved` where its entry already sits, then repairs the envelopes.
///
/// An entry the hierarchy has never seen — a fragment indexed for the first time by the very walk
/// that moved it — has no leaf to be written into and takes the ordinary placement instead.
pub(crate) fn settle(tree: &mut RTree, moved: &[Carried], homes: &mut Homes) {
    let mut touched: Vec<usize> = Vec::with_capacity(moved.len());
    for carried in moved {
        let (key, bounds) = (carried.frag, carried.bounds);
        let Some((leaf, slot)) = home_slot(tree, key, homes) else {
            tree.place(key, bounds, homes);
            continue;
        };
        tree.nodes[leaf].entries[slot].1 = bounds;
        tree.placements.in_place += 1;
        counter::bump(Counter::HitEntriesMovedInPlace);
        touched.push(leaf);
    }
    repair(tree, touched);
}

/// Where one entry sits, if the hierarchy is holding it in a leaf that still lists it.
fn home_slot(tree: &RTree, key: FragKey, homes: &Homes) -> Option<(usize, usize)> {
    let leaf = tree.leaf_of(key, homes)?;
    let slot = tree.nodes.get(leaf)?.slot_of(key)?;
    Some((leaf, slot))
}

/// Recomputes the envelope of every node in `level` and of every node above them, once each.
///
/// Level by level rather than by climbing from each leaf. A scroll touches most of the leaves under
/// one branch, and climbing from each of them would recompute that branch once per leaf; sorting
/// and deduplicating each level is what makes it once per node instead. A node whose union did not
/// move stops the climb, because a node above it is a union of unions.
fn repair(tree: &mut RTree, mut level: Vec<usize>) {
    let mut above: Vec<usize> = Vec::new();
    while !level.is_empty() {
        level.sort_unstable();
        level.dedup();
        above.clear();
        for &index in &level {
            let envelope = union_below(tree, index);
            if tree.nodes[index].envelope == envelope {
                continue;
            }
            tree.nodes[index].envelope = envelope;
            if let Some(parent) = tree.nodes[index].parent {
                above.push(parent);
            }
        }
        core::mem::swap(&mut level, &mut above);
    }
}

/// The union of whatever one node holds: its entries when it is a leaf, its children's envelopes
/// when it is not.
fn union_below(tree: &RTree, index: usize) -> Rect<DevicePx, Device> {
    let node = &tree.nodes[index];
    match &node.children {
        None => envelope_of(node.entries.iter().map(|entry| entry.1)),
        Some(children) => envelope_of(children.iter().map(|child| tree.nodes[*child].envelope)),
    }
}
