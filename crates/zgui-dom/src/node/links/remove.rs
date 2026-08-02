//! Taking a node out of a child list.
//!
//! Unlinking is the mirror of linking and has one extra obligation: the node's own links are
//! cleared as well as its neighbours'. Leaving them behind would make a detached node look like a
//! child of a parent that no longer lists it, and every liveness test in the crate is "is this
//! still my parent's child" — a stale parent link answers yes.

use crate::arena::store::DocumentStore;
use crate::id::node_key::{NodeIndex, OptIndex};
use crate::node::links::note_shape_change;

/// Takes `child` out of its parent's child list, leaving it detached with no links of its own.
///
/// Returns the parent it was taken from, or [`None`] if it had none. Its own children are
/// untouched: a removed subtree keeps its shape, which is what lets the same subtree be linked in
/// again somewhere else.
///
/// # Panics
///
/// Panics if `child` names no live node of `store`.
pub(crate) fn unlink(store: &DocumentStore, child: NodeIndex) -> Option<NodeIndex> {
    let record = store.core(child);
    let parent = record.parent()?;
    let parent_record = store.core(parent);

    let previous = record.prev_sibling.get();
    let next = record.next_sibling.get();
    // Before the links go: a dirty-child run ending at this node has to re-anchor onto a sibling
    // that is still here, or it names a child list it no longer describes.
    parent_record
        .dirty_children()
        .note_unlinked(child, previous.get(), next.get());
    match previous.get() {
        Some(previous) => store.core(previous).next_sibling.set(next),
        None => parent_record.first_child.set(next),
    }
    match next.get() {
        Some(next) => store.core(next).prev_sibling.set(previous),
        None => parent_record.last_child.set(previous),
    }
    parent_record
        .child_count
        .set(parent_record.child_count.get().saturating_sub(1));

    if record.kind().in_element_chain() {
        let previous_element = record.prev_element.get();
        let next_element = record.next_element.get();
        match previous_element.get() {
            Some(previous) => store.core(previous).next_element.set(next_element),
            None => parent_record.first_element_child.set(next_element),
        }
        if let Some(next) = next_element.get() {
            store.core(next).prev_element.set(previous_element);
        }
    }

    record.parent.set(OptIndex::NONE);
    record.prev_sibling.set(OptIndex::NONE);
    record.next_sibling.set(OptIndex::NONE);
    record.prev_element.set(OptIndex::NONE);
    record.next_element.set(OptIndex::NONE);

    note_shape_change(store, parent);
    Some(parent)
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::id::node_key::NodeIndex;
    use crate::node::kind::NodeKind;
    use crate::node::links::{last_element_child, link_before, unlink};

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
    fn removing_the_middle_element_joins_its_neighbours_on_both_chains() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let first = document.append(root, NodeKind::Element, ElementName::new("a"));
        let middle = document.append(root, NodeKind::Element, ElementName::new("b"));
        let last = document.append(root, NodeKind::Element, ElementName::new("c"));

        assert_eq!(unlink(document.store(), middle), Some(root));

        assert_eq!(elements(&document, root), vec![first, last]);
        assert_eq!(document.store().core(first).next_sibling(), Some(last));
        assert_eq!(document.store().core(last).prev_sibling(), Some(first));
        assert_eq!(document.store().core(root).child_count(), 2);
        assert_eq!(document.store().core(middle).parent(), None);
    }

    #[test]
    fn removing_the_only_element_empties_both_ends() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let only = document.append(root, NodeKind::Element, ElementName::new("a"));
        unlink(document.store(), only);

        assert_eq!(document.store().core(root).first_child(), None);
        assert_eq!(document.store().core(root).last_child(), None);
        assert_eq!(document.store().core(root).first_element_child(), None);
        assert_eq!(last_element_child(document.store(), root), None);
    }

    #[test]
    fn a_removed_subtree_keeps_its_own_shape_and_can_be_linked_in_again() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let host = document.append(root, NodeKind::Element, ElementName::new("a"));
        let inner = document.append(host, NodeKind::Element, ElementName::new("b"));
        let other = document.append(root, NodeKind::Element, ElementName::new("c"));

        unlink(document.store(), host);
        assert_eq!(
            document.store().core(host).first_element_child(),
            Some(inner)
        );
        assert_eq!(document.store().core(inner).parent(), Some(host));

        link_before(document.store(), other, host, None);
        assert_eq!(elements(&document, other), vec![host]);
        assert_eq!(document.store().core(inner).parent(), Some(host));
    }

    #[test]
    fn removing_a_node_with_no_parent_reports_that_it_had_none() {
        let mut document = Document::new();
        let loose = document.detached(NodeKind::Element, ElementName::new("a"));
        assert_eq!(unlink(document.store(), loose), None);
    }
}
