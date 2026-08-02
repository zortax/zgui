//! Reading the entries under a point back out.

use zgui_geom::{Device, DevicePx, Point};

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::RTree;

/// Collects every entry under `point` from one subtree.
///
/// A node whose envelope misses the point is dismissed whole, which is the entire value of the
/// hierarchy — and the reason an envelope is only ever allowed to be too large, never too small.
pub(crate) fn collect(
    tree: &RTree,
    at: usize,
    point: Point<DevicePx, Device>,
    out: &mut impl Extend<FragKey>,
) {
    let node = tree.node(at);
    if !node.envelope.contains(point) {
        return;
    }
    match &node.children {
        None => out.extend(
            node.entries
                .iter()
                .filter(|(_, bounds)| bounds.contains(point))
                .map(|(key, _)| *key),
        ),
        Some(children) => {
            for child in children {
                collect(tree, *child, point, out);
            }
        }
    }
}
