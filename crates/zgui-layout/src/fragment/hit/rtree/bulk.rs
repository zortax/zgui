//! Filling an empty tree in one pass.
//!
//! A bulk rebuild of the index knows every entry before it files the first one, so sorting them
//! costs less than descending the tree once per entry, and the leaves come out with neighbours in
//! them. The entries are sorted across, cut into columns, each column is sorted down and cut into
//! leaves, and the nodes above are made one level at a time.
//!
//! Leaves are left with room in them, so the single insertions that follow a rebuild do not split
//! every leaf they reach.

use zgui_geom::{Device, DevicePx, Rect};

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::home::Homes;
use crate::fragment::hit::rtree::node::Node;
use crate::fragment::hit::rtree::{MAX_ENTRIES, RTree, envelope_of};

/// How many members a node is filled to.
const FILL: usize = MAX_ENTRIES * 3 / 4;

/// The middle of a rectangle across.
fn across(bounds: &Rect<DevicePx, Device>) -> f32 {
    bounds.origin.x.0 + bounds.size.width.0 / 2.0
}

/// The middle of a rectangle down.
fn down(bounds: &Rect<DevicePx, Device>) -> f32 {
    bounds.origin.y.0 + bounds.size.height.0 / 2.0
}

/// Files `entries` in `tree`, which holds nothing yet.
pub(crate) fn fill(
    tree: &mut RTree,
    mut entries: Vec<(FragKey, Rect<DevicePx, Device>)>,
    homes: &mut Homes,
) {
    debug_assert!(tree.root.is_none(), "a tree is filled once, while empty");
    tree.len = entries.len();
    if entries.is_empty() {
        return;
    }
    let leaves = entries.len().div_ceil(FILL);
    let columns = leaves.isqrt().max(1);
    let per_column = entries.len().div_ceil(columns);
    entries.sort_unstable_by(|one, two| across(&one.1).total_cmp(&across(&two.1)));

    tree.nodes.reserve(leaves + leaves / (FILL - 1) + 1);
    let mut level = Vec::with_capacity(leaves);
    for column in entries.chunks_mut(per_column) {
        column.sort_unstable_by(|one, two| down(&one.1).total_cmp(&down(&two.1)));
        for group in column.chunks(FILL) {
            let envelope = envelope_of(group.iter().map(|entry| entry.1));
            let leaf = tree.push(Node::leaves(group.to_vec(), envelope));
            for entry in group {
                tree.file(entry.0, leaf, homes);
            }
            level.push(leaf);
        }
    }
    while level.len() > 1 {
        let mut above = Vec::with_capacity(level.len().div_ceil(FILL));
        for group in level.chunks(FILL) {
            let envelope = envelope_of(group.iter().map(|&node| tree.nodes[node].envelope));
            let node = tree.push(Node::internal(group.to_vec(), envelope));
            for &child in group {
                tree.nodes[child].parent = Some(node);
            }
            above.push(node);
        }
        level = above;
    }
    tree.root = level.first().copied();
}
