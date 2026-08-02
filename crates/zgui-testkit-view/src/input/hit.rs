//! Which node a point lands on, given the boxes a test declared.

use zgui_geom::{Device, DevicePx, Point};
use zgui_view::NodeId;

use crate::dom::RecordingDom;
use crate::host::ScriptedHost;

/// The node under `point`, or nothing when the point is over none.
///
/// Three rules, and all three are stated because a harness that guessed at them would make
/// component tests depend on the guess:
///
/// * a node with no declared box is not hit, and its descendants are still considered — a test
///   declares only the boxes it wants aimed at, and a wrapper without one is not in the way;
/// * a descendant is preferred to its ancestor, so a press on a button inside a toolbar lands on
///   the button;
/// * where two boxes at the same depth overlap, the one **later in document order** wins, because
///   that is the one a painter would have drawn last.
///
/// This is a test harness's answer, not a layout engine's: there is no painting order here to read
/// and no stacking contexts to respect, so document order is the closest honest approximation and
/// a test that needs more than that needs a real frame.
pub fn topmost(
    dom: &RecordingDom,
    host: &ScriptedHost,
    root: NodeId,
    point: Point<DevicePx, Device>,
) -> Option<NodeId> {
    let mut found = None;
    visit(dom, host, root, point, &mut found);
    found
}

/// Walks the subtree in document order, keeping the last node that covers the point.
fn visit(
    dom: &RecordingDom,
    host: &ScriptedHost,
    node: NodeId,
    point: Point<DevicePx, Device>,
    found: &mut Option<NodeId>,
) {
    if covers(host, node, point) {
        *found = Some(node);
    }
    for child in dom.tree().children(node) {
        visit(dom, host, child, point, found);
    }
}

/// Whether the box a test declared for `node` contains `point`.
fn covers(host: &ScriptedHost, node: NodeId, point: Point<DevicePx, Device>) -> bool {
    use zgui_view::ViewHost;
    host.border_box(node)
        .is_some_and(|bounds| bounds.contains(point))
}
