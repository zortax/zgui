//! Putting one entry in, and the splits that follow when a node fills up.

use zgui_geom::{Device, DevicePx, Rect};

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::home::Homes;
use crate::fragment::hit::rtree::node::{Node, split};
use crate::fragment::hit::rtree::{MAX_ENTRIES, RTree, area, envelope_of};

/// Inserts into one subtree, returning a new sibling if that subtree had to split.
///
/// Every node the descent passes through takes the new rectangle into its envelope on the way down,
/// so the hierarchy above the entry contains it before the entry is written.
pub(crate) fn into(
    tree: &mut RTree,
    at: usize,
    key: FragKey,
    bounds: Rect<DevicePx, Device>,
    homes: &mut Homes,
) -> Option<usize> {
    tree.nodes[at].envelope = tree.nodes[at].envelope.union(bounds);
    if tree.nodes[at].children.is_none() {
        return into_leaf(tree, at, key, bounds, homes);
    }
    let chosen = choose(tree, at, bounds);
    let sibling = into(tree, chosen, key, bounds, homes)?;
    adopt(tree, at, sibling);
    let full = tree.nodes[at]
        .children
        .as_ref()
        .is_some_and(|children| children.len() > MAX_ENTRIES);
    if !full {
        return None;
    }
    Some(divide_children(tree, at))
}

/// Writes one entry into a leaf, dividing it when it overflows.
fn into_leaf(
    tree: &mut RTree,
    at: usize,
    key: FragKey,
    bounds: Rect<DevicePx, Device>,
    homes: &mut Homes,
) -> Option<usize> {
    tree.nodes[at].entries.push((key, bounds));
    tree.file(key, at, homes);
    if tree.nodes[at].entries.len() <= MAX_ENTRIES {
        return None;
    }
    let taken = core::mem::take(&mut tree.nodes[at].entries);
    let (kept, moved) = split(taken, |entry| entry.1);
    tree.nodes[at].envelope = envelope_of(kept.iter().map(|entry| entry.1));
    tree.nodes[at].entries = kept;
    let envelope = envelope_of(moved.iter().map(|entry| entry.1));
    let sibling = tree.push(Node::leaves(moved, envelope));
    // The entries that left have a new home, and an entry filed under the wrong leaf is one that
    // can never be found again by name — which is a rectangle answering hits for ever.
    for slot in 0..tree.nodes[sibling].entries.len() {
        let moved_key = tree.nodes[sibling].entries[slot].0;
        tree.file(moved_key, sibling, homes);
    }
    Some(sibling)
}

/// Hangs a new sibling below one node.
fn adopt(tree: &mut RTree, at: usize, sibling: usize) {
    tree.nodes[sibling].parent = Some(at);
    if let Some(children) = tree.nodes[at].children.as_mut() {
        children.push(sibling);
    }
}

/// Divides an overfull interior node, returning the half that left.
fn divide_children(tree: &mut RTree, at: usize) -> usize {
    let taken = tree.nodes[at]
        .children
        .take()
        .expect("an internal node has children");
    let envelopes: Vec<(usize, Rect<DevicePx, Device>)> = taken
        .into_iter()
        .map(|index| (index, tree.nodes[index].envelope))
        .collect();
    let (kept, moved) = split(envelopes, |entry| entry.1);
    tree.nodes[at].envelope = envelope_of(kept.iter().map(|entry| entry.1));
    tree.nodes[at].children = Some(kept.into_iter().map(|entry| entry.0).collect());
    let envelope = envelope_of(moved.iter().map(|entry| entry.1));
    let sibling = tree.push(Node::internal(
        moved.into_iter().map(|entry| entry.0).collect(),
        envelope,
    ));
    let count = tree.nodes[sibling].children.as_ref().map_or(0, Vec::len);
    for slot in 0..count {
        let child = tree.nodes[sibling]
            .children
            .as_ref()
            .expect("an internal node has children")[slot];
        tree.nodes[child].parent = Some(sibling);
    }
    sibling
}

/// The child whose rectangle grows least by taking `bounds`.
fn choose(tree: &RTree, at: usize, bounds: Rect<DevicePx, Device>) -> usize {
    let children = tree.nodes[at]
        .children
        .as_ref()
        .expect("an internal node has children");
    let mut best = children[0];
    let mut best_growth = f32::INFINITY;
    let mut best_area = f32::INFINITY;
    for &child in children {
        let envelope = tree.nodes[child].envelope;
        let held = area(envelope);
        let growth = area(envelope.union(bounds)) - held;
        if growth < best_growth || (growth == best_growth && held < best_area) {
            best = child;
            best_growth = growth;
            best_area = held;
        }
    }
    best
}
