//! Keeping every node's rectangle equal to the union of what is below it.

use zgui_geom::{Device, DevicePx, Rect};

use crate::fragment::hit::rtree::{RTree, envelope_of};

/// Recomputes envelopes from one node up to the root, dropping children that emptied.
///
/// The climb stops at the first node whose envelope the change did not alter: an envelope is the
/// union of what is below it, so a node whose union is unchanged has ancestors whose unions are
/// unchanged too.
///
/// Both directions go through here. A removal makes a node's union smaller and a rewritten entry
/// can make it larger, and in neither case may an ancestor be left holding a rectangle that does
/// not contain what is underneath it — an envelope that is too small is a subtree dismissed for a
/// point that really is inside it, which is a click that lands on nothing.
pub(crate) fn refresh(tree: &mut RTree, from: usize) {
    let mut at = Some(from);
    while let Some(index) = at {
        let held = tree.nodes[index].envelope;
        let envelope = if tree.nodes[index].children.is_none() {
            envelope_of(tree.nodes[index].entries.iter().map(|entry| entry.1))
        } else {
            prune(tree, index)
        };
        tree.nodes[index].envelope = envelope;
        if envelope == held {
            return;
        }
        at = tree.nodes[index].parent;
    }
}

/// Drops one interior node's emptied children and reports what remains of its envelope.
fn prune(tree: &mut RTree, at: usize) -> Rect<DevicePx, Device> {
    let mut children = tree.nodes[at]
        .children
        .take()
        .expect("an internal node has children");
    children.retain(|child| !tree.nodes[*child].is_empty());
    let envelope = envelope_of(children.iter().map(|child| tree.nodes[*child].envelope));
    tree.nodes[at].children = Some(children);
    envelope
}
