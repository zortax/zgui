//! Taking one entry out, by name rather than by searching for its rectangle.

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::home::Homes;
use crate::fragment::hit::rtree::{RTree, envelope};

/// Takes one entry out and tightens the envelopes above it.
///
/// Returns whether it was there. The entry is found through the home index, so a caller does not
/// have to hand over the rectangle it was inserted with — which is what makes a removal cost the
/// depth of the tree rather than a search from the root, and what makes it impossible to leave a
/// name behind by asking for the wrong rectangle.
pub(crate) fn take(tree: &mut RTree, key: FragKey, homes: &mut Homes) -> bool {
    let Some(leaf) = tree.leaf_of(key, homes) else {
        return false;
    };
    let Some(slot) = tree.nodes[leaf].slot_of(key) else {
        // The home index named a leaf that does not hold it. Nothing can be removed, and leaving
        // the stale name behind would make every later lookup for this key read the wrong node.
        homes.remove(key);
        return false;
    };
    tree.nodes[leaf].entries.swap_remove(slot);
    homes.remove(key);
    tree.len -= 1;
    envelope::refresh(tree, leaf);
    if tree.root.is_some_and(|root| tree.nodes[root].is_empty()) {
        tree.reset();
    }
    true
}
