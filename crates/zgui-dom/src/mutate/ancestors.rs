//! Telling a node's ancestors that there is work below them.
//!
//! The style engine descends only where something says there is work, and it does not raise that
//! signal on the embedder's behalf: a change made from outside the engine reaches nothing unless
//! the ancestors of the changed node are told. Missing this step produces no error, no log and no
//! counter movement — the traversal simply stops above the change and the element keeps the style
//! it had. It is the single most likely cause of "my `:hover` rule does nothing".
//!
//! # A subtree built while detached needs more than a mark
//!
//! Everything marked on a node while it had no parent stopped at that node: there were no ancestors
//! to tell. Linking it in does not fix that by itself, and marking the node again cannot either —
//! marking returns at the node's own word, which already owes those obligations, before it reaches
//! a single ancestor. So the obligations a detached subtree accumulated are *spliced* into its new
//! parent's chain as a separate step, and an inserted subtree that skips it is invisible to every
//! stage of the frame.

use zgui_bits::Dirty;

use crate::arena::store::DocumentStore;
use crate::dirty::propagate;
use crate::id::node_key::NodeIndex;

/// Records that `node` owes `bits`, and that every ancestor must descend into it.
///
/// # Panics
///
/// Panics if `node` names no live node of `store`.
pub(crate) fn mark(store: &mut DocumentStore, node: NodeIndex, bits: Dirty) {
    propagate::mark(store, node, bits);
}

/// Folds everything `child` and its descendants already owe into `child`'s ancestors.
///
/// Called after a subtree is linked in, with `child` the root of what was linked. Its own word is
/// left exactly as it is; only the chain above it learns what is below.
///
/// # Panics
///
/// Panics if `child` names no live node of `store`.
pub(crate) fn splice(store: &mut DocumentStore, child: NodeIndex) {
    let (own, subtree) = store.core(child).dirty().get();
    propagate::propagate(store, child, own | subtree);
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_interned::ElementName;

    use super::{mark, splice};
    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn a_subtree_marked_while_detached_reaches_the_root_once_it_is_spliced() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let host = document.detached(NodeKind::Element, ElementName::new("body"));
        let inner = document.detached(NodeKind::Element, ElementName::new("item"));
        crate::node::links::link_before(document.store(), host, inner, None);
        mark(document.store_mut(), inner, Dirty::RESTYLE);
        assert!(
            !document
                .store()
                .core(root)
                .dirty()
                .subtree()
                .contains(Dirty::RESTYLE),
            "there was no path from the root to reach"
        );

        crate::node::links::link_before(document.store(), root, host, None);
        splice(document.store_mut(), host);

        assert!(
            document
                .store()
                .core(root)
                .dirty()
                .subtree()
                .contains(Dirty::RESTYLE)
        );
        let named: Vec<_> = document
            .store()
            .core(root)
            .dirty_children()
            .iter(document.store(), root)
            .collect();
        assert_eq!(named, vec![host]);
    }

    #[test]
    fn marking_the_spliced_node_again_would_not_have_done_it() {
        // The property the splice exists for, stated as its own case: the second mark returns at
        // the node's own word and tells no ancestor anything.
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let host = document.detached(NodeKind::Element, ElementName::new("body"));
        mark(document.store_mut(), host, Dirty::RESTYLE);
        crate::node::links::link_before(document.store(), root, host, None);

        mark(document.store_mut(), host, Dirty::RESTYLE);
        assert!(
            !document
                .store()
                .core(root)
                .dirty()
                .subtree()
                .contains(Dirty::RESTYLE)
        );

        splice(document.store_mut(), host);
        assert!(
            document
                .store()
                .core(root)
                .dirty()
                .subtree()
                .contains(Dirty::RESTYLE)
        );
    }
}
