//! Finding the box a splice replaces, and the box that positions what is inside it.

use zgui_dom::side::BoxKey;
use zgui_dom::{Document, NodeIndex};

use crate::boxtree::absolute::establishes_containing_block;
use crate::boxtree::classify::classify;
use crate::node::kind::BoxKind;
use crate::tree::store::LayoutStore;

/// The one box an element generated for itself, if it generated exactly one.
///
/// An element's box list holds more than its own box: a `::before`, a `::after`, a list item's mark
/// and the runs of text inside those all name the same element as their source. What a splice
/// replaces is the element's own box, and the rest are inside it.
///
/// `None` when there is no such box or more than one, both of which mean the tree does not have the
/// shape this splice assumes.
pub(super) fn primary_box(
    store: &LayoutStore,
    document: &Document,
    index: NodeIndex,
) -> Option<BoxKey> {
    let key = document.store().key_of(index);
    let mut found = None;
    for &box_key in store.boxes_of(key) {
        let node = store.get(box_key)?;
        if node.pseudo.is_some() || node.kind != BoxKind::Element {
            continue;
        }
        if found.replace(box_key).is_some() {
            return None;
        }
    }
    found
}

/// The box that positions an out-of-flow box written inside `from`.
///
/// Walked over the boxes that are there rather than derived a second time from the document: the
/// build resolves a containing block while it descends, and an answer recomputed from styles here
/// would agree with that one only until one of the two learned about something the other did not.
///
/// The walk starts *at* `from` and follows layout parents, which is the same chain the build
/// descended: an out-of-flow ancestor is in it, because a box is taken out of its writer's layout
/// list and not out of its own children's.
pub(super) fn containing_block(store: &LayoutStore, from: BoxKey) -> Option<BoxKey> {
    let mut next = Some(from);
    while let Some(key) = next {
        let node = store.get(key)?;
        if establishes_containing_block(classify(&node.style).positioned, node.fc) {
            return Some(key);
        }
        next = node.parent;
    }
    None
}
