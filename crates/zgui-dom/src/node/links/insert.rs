//! Linking a node into a child list.
//!
//! One function does the work and the append case is a call to it with no reference node, because
//! the two differ only in where the search for the surrounding element siblings starts. Keeping
//! them separate is how the element-only chain drifts from the plain one: appending is the common
//! case, so an append-only implementation is the one that gets tested, and the insert path then
//! gets the element links subtly wrong in the presence of text nodes.

use crate::arena::store::DocumentStore;
use crate::id::node_key::{NodeIndex, OptIndex};
use crate::node::links::{following_element, note_shape_change, previous_element};

/// Links `child` onto the end of `parent`'s child list, maintaining both chains.
///
/// # Panics
///
/// Panics if either index names no live node of `store`.
pub(crate) fn append_child(store: &DocumentStore, parent: NodeIndex, child: NodeIndex) {
    link_before(store, parent, child, None);
}

/// Links `child` into `parent`'s child list ahead of `before`, or at the end if there is none.
///
/// `child` must not already be linked anywhere: taking a node out is a separate operation, because
/// removal has obligations of its own that a silent relink would skip.
///
/// # Panics
///
/// Panics if any index names no live node of `store`, or if `before` is not a child of `parent`.
pub(crate) fn link_before(
    store: &DocumentStore,
    parent: NodeIndex,
    child: NodeIndex,
    before: Option<NodeIndex>,
) {
    let parent_record = store.core(parent);
    let child_record = store.core(child);
    debug_assert!(
        child_record.parent().is_none(),
        "a node is unlinked from wherever it was before it is linked somewhere else"
    );
    if let Some(before) = before {
        assert_eq!(
            store.core(before).parent(),
            Some(parent),
            "the node to insert before has to be a child of the parent being inserted into"
        );
    }

    // The plain chain, from the two neighbours the reference node sits between.
    let next = OptIndex::from_option(before);
    let previous = match before {
        Some(before) => store.core(before).prev_sibling.get(),
        None => parent_record.last_child.get(),
    };
    child_record.parent.set(OptIndex::some(parent));
    child_record.prev_sibling.set(previous);
    child_record.next_sibling.set(next);
    match previous.get() {
        Some(previous) => store.core(previous).next_sibling.set(OptIndex::some(child)),
        None => parent_record.first_child.set(OptIndex::some(child)),
    }
    match next.get() {
        Some(next) => store.core(next).prev_sibling.set(OptIndex::some(child)),
        None => parent_record.last_child.set(OptIndex::some(child)),
    }
    parent_record
        .child_count
        .set(parent_record.child_count.get() + 1);

    // The element-only chain, whose neighbours are found by stepping past whatever is not an
    // element. This is the one place that walk is paid for; every selector test afterwards is a
    // single link.
    if child_record.kind().in_element_chain() {
        let previous_element = previous_element(store, previous);
        let next_element = following_element(store, next);
        child_record
            .prev_element
            .set(OptIndex::from_option(previous_element));
        child_record
            .next_element
            .set(OptIndex::from_option(next_element));
        match previous_element {
            Some(previous) => store.core(previous).next_element.set(OptIndex::some(child)),
            None => parent_record.first_element_child.set(OptIndex::some(child)),
        }
        if let Some(next) = next_element {
            store.core(next).prev_element.set(OptIndex::some(child));
        }
    }

    note_shape_change(store, parent);
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::id::node_key::NodeIndex;
    use crate::node::kind::NodeKind;
    use crate::node::links::{last_element_child, link_before};

    /// The plain child chain of `parent`, front to back.
    fn plain(document: &Document, parent: NodeIndex) -> Vec<NodeIndex> {
        let mut out = Vec::new();
        let mut current = document.store().core(parent).first_child();
        while let Some(index) = current {
            out.push(index);
            current = document.store().core(index).next_sibling();
        }
        out
    }

    /// The element-only chain of `parent`, front to back.
    fn elements(document: &Document, parent: NodeIndex) -> Vec<NodeIndex> {
        let mut out = Vec::new();
        let mut current = document.store().core(parent).first_element_child();
        while let Some(index) = current {
            out.push(index);
            current = document.store().core(index).next_element();
        }
        out
    }

    #[test]
    fn inserting_before_a_text_node_still_finds_the_element_neighbours() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let first = document.append(root, NodeKind::Element, ElementName::new("a"));
        let gap = document.append(root, NodeKind::Text, ElementName::new("#text"));
        let last = document.append(root, NodeKind::Element, ElementName::new("b"));

        let fresh = document.detached(NodeKind::Element, ElementName::new("c"));
        link_before(document.store(), root, fresh, Some(gap));

        assert_eq!(plain(&document, root), vec![first, fresh, gap, last]);
        assert_eq!(elements(&document, root), vec![first, fresh, last]);
        assert_eq!(
            document.store().core(last).prev_element(),
            Some(fresh),
            "the element after the text node has to learn about its new predecessor"
        );
    }

    #[test]
    fn prepending_moves_the_first_element_child() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let old_first = document.append(root, NodeKind::Element, ElementName::new("a"));

        let fresh = document.detached(NodeKind::Element, ElementName::new("b"));
        link_before(document.store(), root, fresh, Some(old_first));

        assert_eq!(
            document.store().core(root).first_element_child(),
            Some(fresh)
        );
        assert_eq!(last_element_child(document.store(), root), Some(old_first));
        assert_eq!(document.store().core(root).child_count(), 2);
    }

    #[test]
    fn appending_a_marker_leaves_the_element_chain_alone() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let only = document.append(root, NodeKind::Element, ElementName::new("a"));
        let marker = document.detached(NodeKind::Marker, ElementName::new("#marker"));
        link_before(document.store(), root, marker, None);

        assert_eq!(elements(&document, root), vec![only]);
        assert_eq!(plain(&document, root), vec![only, marker]);
        assert_eq!(last_element_child(document.store(), root), Some(only));
    }
}
