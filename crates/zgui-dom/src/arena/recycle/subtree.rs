//! Finding the records a frame's removals left behind, and dropping them.

use crate::arena::document::Document;
use crate::id::node_key::NodeIndex;

/// Drops every subtree that was removed during the frame and is still out of the document.
///
/// Idempotent per node: a node that was removed, put back and removed again is named twice, and
/// dropping it twice is the same as dropping it once — the arena refuses the second removal and the
/// columns have nothing left to clear.
pub(super) fn drop_detached(document: &mut Document) {
    let dropping = collect(document);
    if dropping.is_empty() {
        return;
    }
    // The pre-change records go before the nodes do, because they are keyed by the element's
    // address and the slot is about to be handed out again at that same address.
    //
    // SAFETY: an exclusive borrow of the document rules out any other reference into the cell.
    unsafe { document.edit_state().batch() }
        .snapshots
        .forget(document.store(), &dropping);

    let store = document.store_mut();
    for node in dropping {
        store.drop_node(node);
    }
}

/// Every node of every subtree the frame removed and did not put back, roots included.
fn collect(document: &mut Document) -> Vec<NodeIndex> {
    // SAFETY: an exclusive borrow of the document rules out any other reference into the cell.
    let mut pending = core::mem::take(&mut unsafe { document.edit_state().batch() }.detached);
    let store = document.store();
    // A root that has a parent again was put back during the frame, and putting it back is
    // something the frame's own stages are entitled to do.
    pending.retain(|node| {
        store
            .try_core(*node)
            .is_some_and(|record| record.parent().is_none())
    });

    let mut dropping = Vec::with_capacity(pending.len());
    while let Some(node) = pending.pop() {
        if let Some(record) = store.try_core(node) {
            // Down the child chain rather than by slot number: a descendant that was moved into
            // the live tree during the frame is no longer on it, and one that was moved *in* is.
            let mut child = record.first_child();
            while let Some(current) = child {
                child = store.core(current).next_sibling();
                pending.push(current);
            }
        }
        dropping.push(node);
    }
    dropping
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::arena::recycle::end_frame;
    use crate::id::node_key::NodeIndex;
    use crate::mutate::filter::EverythingMatters;
    use crate::node::kind::NodeKind;

    /// Whether the store still holds a record for `node`.
    fn holds(document: &Document, node: NodeIndex) -> bool {
        document.store().try_core(node).is_some()
    }

    /// A row under `root` with a text node under it.
    fn branch(document: &mut Document, root: NodeIndex) -> (NodeIndex, NodeIndex) {
        let row = document.append(root, NodeKind::Element, ElementName::new("li"));
        let text = document.append(row, NodeKind::Text, ElementName::new("#text"));
        (row, text)
    }

    /// A document with a root element, and the root.
    fn document() -> (Document, NodeIndex) {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        (document, root)
    }

    #[test]
    fn a_removed_subtree_is_dropped_whole_when_the_frame_ends() {
        let (mut document, root) = document();
        let (row, text) = branch(&mut document, root);

        document
            .edit(&EverythingMatters, |batch| batch.remove(row))
            .expect("the document is not poisoned");
        assert!(holds(&document, row), "still readable within the frame");
        assert!(holds(&document, text));

        end_frame(&mut document);
        assert!(!holds(&document, row));
        assert!(!holds(&document, text), "the descendant went with its root");
        assert!(holds(&document, root));
    }

    #[test]
    fn a_descendant_moved_out_of_a_removed_subtree_is_kept() {
        let (mut document, root) = document();
        let (row, text) = branch(&mut document, root);

        document
            .edit(&EverythingMatters, |batch| {
                batch.remove(row);
                batch.insert_before(root, text, None);
            })
            .expect("the document is not poisoned");

        end_frame(&mut document);
        assert!(!holds(&document, row));
        assert!(
            holds(&document, text),
            "it is reached from its new parent, not from the root it left"
        );
    }

    #[test]
    fn a_live_node_moved_into_a_removed_subtree_goes_with_it() {
        let (mut document, root) = document();
        let (row, _) = branch(&mut document, root);
        let other = document.append(root, NodeKind::Element, ElementName::new("li"));

        document
            .edit(&EverythingMatters, |batch| {
                batch.remove(row);
                batch.insert_before(row, other, None);
            })
            .expect("the document is not poisoned");

        end_frame(&mut document);
        assert!(
            !holds(&document, other),
            "nothing in the document leads to it any more"
        );
    }

    #[test]
    fn removing_the_same_node_twice_in_one_frame_drops_it_once() {
        let (mut document, root) = document();
        let (row, _) = branch(&mut document, root);

        document
            .edit(&EverythingMatters, |batch| {
                batch.remove(row);
                batch.insert_before(root, row, None);
                batch.remove(row);
            })
            .expect("the document is not poisoned");

        end_frame(&mut document);
        assert!(!holds(&document, row));
        assert_eq!(document.len(), 2, "the document node and the root");
    }

    #[test]
    fn a_subtree_the_damage_stage_has_already_read_is_still_dropped() {
        // The stage that works out what to repaint *takes* the removed roots during the frame, and
        // it is the only consumer of that list. A recycling pass reading the same list would find
        // it emptied and drop nothing at all — so every removal made by a frame that produced any
        // damage would be kept for the life of the document, with the damage still correct, the
        // tree still right and nothing anywhere to notice by. Hence two lists and this case.
        let (mut document, root) = document();
        let (row, text) = branch(&mut document, root);

        document
            .edit(&EverythingMatters, |batch| batch.remove(row))
            .expect("the document is not poisoned");
        assert_eq!(
            document.take_removed(),
            vec![row],
            "the damage stage read the removed roots, and emptied the list it read"
        );

        end_frame(&mut document);
        assert!(!holds(&document, row));
        assert!(!holds(&document, text));
        assert_eq!(document.len(), 2, "the document node and the root");
    }

    #[test]
    fn a_pre_change_record_does_not_outlive_the_element_it_describes() {
        // The record is keyed by the element's address, and the arena hands a returned slot back
        // out at the address it always had. A record left behind is therefore found for the next
        // occupant, which is then invalidated against the previous occupant's state — no panic, no
        // counter, one element wearing a style no rule justifies.
        let (mut document, root) = document();
        let gone = document.append(root, NodeKind::Element, ElementName::new("li"));

        document
            .edit(&EverythingMatters, |batch| {
                batch.set_state(gone, zgui_vocab::UiState::HOVER, true);
                batch.remove(gone);
            })
            .expect("the document is not poisoned");
        assert_eq!(document.pending_snapshots(), 1, "a record was taken");

        end_frame(&mut document);
        let fresh = document.append(root, NodeKind::Element, ElementName::new("li"));
        assert_eq!(fresh, gone, "the slot, and so the address, came back");

        let snapshots = document.take_snapshots();
        assert!(
            snapshots.of(document.store(), fresh).is_none(),
            "the record the removed element left behind was found for its successor"
        );
    }
}
