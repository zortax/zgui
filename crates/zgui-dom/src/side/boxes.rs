//! Which boxes a node generates.
//!
//! A node and a box are not one-to-one, and CSS forces that in seven separate ways: a hidden node
//! generates none, a node whose children reparent onto its own parent generates none, a run of
//! inline siblings shares one anonymous box, a block inside an inline splits into three, and
//! generated content produces boxes with no node at all. So the document records a *list* of boxes
//! per node, in document order, and the list is usually one entry long — which is why it is a small
//! inline vector rather than a heap allocation.
//!
//! The record for a box is the layout tree's, not the document's. What the document holds is the
//! name of one.

use smallvec::SmallVec;
use zgui_arena::Key;

/// What a [`BoxKey`] names: one box of the layout tree.
///
/// The document stores keys to boxes and never stores a box, so this is the *name* of a box rather
/// than the record itself; the record, its formatting context and its two child lists belong to
/// layout. Declaring the name here is what lets the document record which boxes a node generated
/// without depending on the tree that owns them.
#[derive(Debug)]
pub enum BoxNode {}

/// A generation-checked name for one box of the layout tree.
pub type BoxKey = Key<BoxNode>;

/// The boxes one node generated, in document order.
///
/// Empty for a node that generates none, one entry for almost everything else, several for a split.
pub type BoxList = SmallVec<[BoxKey; 1]>;

#[cfg(test)]
mod tests {
    use zgui_arena::{DomainId, Generation};

    use super::{BoxKey, BoxList};

    #[test]
    fn the_usual_one_box_list_needs_no_heap_allocation() {
        let mut boxes = BoxList::new();
        assert!(boxes.is_empty());
        boxes.push(BoxKey::new(7, Generation::FIRST, DomainId::FIRST));
        assert!(!boxes.spilled(), "one box fits inline");
        assert_eq!(boxes.len(), 1);
    }

    #[test]
    fn a_split_records_every_box_in_document_order() {
        let mut boxes = BoxList::new();
        for index in 0..3 {
            boxes.push(BoxKey::new(index, Generation::FIRST, DomainId::FIRST));
        }
        assert_eq!(
            boxes.iter().map(|key| key.index()).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }
}
