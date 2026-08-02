//! `order`, which no layout algorithm here implements.
//!
//! The property moves a flex or grid item within its container without moving it in the document.
//! So it is applied here, once, by sorting the layout child list — leaving the paint child list in
//! document order, which is what painting, hit testing and accessible geometry read.

use zgui_dom::side::BoxKey;

use crate::tree::store::LayoutStore;

/// Sorts a container's layout children by `order`, keeping equal values in document order.
///
/// The sort is stable, which is the whole specification: items with the same `order` keep their
/// document order relative to one another.
pub fn apply(store: &LayoutStore, children: &mut [BoxKey]) {
    if children.len() < 2 {
        return;
    }
    let ordered = |key: &BoxKey| {
        store
            .get(*key)
            .map_or(0, |node| node.style.get_position().order)
    };
    if children
        .windows(2)
        .all(|pair| ordered(&pair[0]) <= ordered(&pair[1]))
    {
        return;
    }
    children.sort_by_key(ordered);
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;
    use zgui_css::StyleDraft;

    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::tree::store::LayoutStore;

    use super::apply;

    /// Inserts a box whose only interesting property is its `order`.
    fn ordered_box(store: &mut LayoutStore, order: i32) -> zgui_dom::side::BoxKey {
        let mut draft = StyleDraft::initial();
        draft.position_group().order = order;
        store.insert(BoxNode::new(
            draft.build(),
            BoxKind::Element,
            FormattingContext::Block,
        ))
    }

    #[test]
    fn items_are_moved_by_their_order_and_ties_keep_document_order() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let first = ordered_box(&mut store, 0);
        let second = ordered_box(&mut store, -1);
        let third = ordered_box(&mut store, 0);
        let fourth = ordered_box(&mut store, 5);
        let mut children = vec![first, second, third, fourth];
        apply(&store, &mut children);
        assert_eq!(children, vec![second, first, third, fourth]);
    }

    #[test]
    fn a_container_whose_items_are_already_in_order_is_left_alone() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let first = ordered_box(&mut store, 0);
        let second = ordered_box(&mut store, 0);
        let mut children = vec![first, second];
        apply(&store, &mut children);
        assert_eq!(children, vec![first, second]);
    }
}
