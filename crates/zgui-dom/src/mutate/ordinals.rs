//! Comparing children by position, once, after every change that invalidated positions.
//!
//! An element's position among its siblings is numbered lazily: a change to the child list records
//! that the numbering is out of date and the first read afterwards pays for one pass over the list.
//! That is what stops appending a thousand rows from costing a thousand renumbers, and it is also
//! what makes "which of these children comes first" a question that must be asked at most once per
//! parent per batch — every ask in between a change and the renumber pays for a full pass.
//!
//! So there is exactly one function here, it takes every candidate at once, and the liveness filter
//! runs before the numbering rather than after: a candidate recorded by one change and unlinked by
//! the next has no position at all, and asking for one would renumber a list that does not contain
//! it.

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;

/// The earliest of `candidates` that is still a child of `parent`, by position.
///
/// [`None`] when the list is empty or nothing in it survived, which is the caller's signal to take
/// the conservative answer rather than a narrower one.
///
/// # Panics
///
/// Panics if `parent` names no live node of `store`.
pub(crate) fn earliest_surviving(
    store: &DocumentStore,
    parent: NodeIndex,
    candidates: &[NodeIndex],
) -> Option<NodeIndex> {
    let mut best: Option<(u32, NodeIndex)> = None;
    for candidate in candidates.iter().copied() {
        let lives_here = store
            .try_core(candidate)
            .is_some_and(|record| record.parent() == Some(parent));
        if !lives_here {
            continue;
        }
        // The first call renumbers the parent's children; every later call in this loop is a load.
        let ordinal = store.ordinal_of(candidate);
        if best.is_none_or(|(held, _)| ordinal < held) {
            best = Some((ordinal, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use super::earliest_surviving;
    use crate::arena::document::Document;
    use crate::id::node_key::NodeIndex;
    use crate::node::kind::NodeKind;

    /// A parent with `width` element children.
    fn row(width: usize) -> (Document, NodeIndex, Vec<NodeIndex>) {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let children = (0..width)
            .map(|_| document.append(root, NodeKind::Element, ElementName::new("row")))
            .collect();
        (document, root, children)
    }

    #[test]
    fn the_earliest_candidate_wins_however_they_were_recorded() {
        let (document, root, children) = row(6);
        let candidates = [children[4], children[1], children[5]];
        assert_eq!(
            earliest_surviving(document.store(), root, &candidates),
            Some(children[1])
        );
    }

    #[test]
    fn a_candidate_that_left_the_parent_is_skipped_rather_than_asked_about() {
        let (document, root, children) = row(6);
        crate::node::links::unlink(document.store(), children[1]);
        let candidates = [children[4], children[1]];
        assert_eq!(
            earliest_surviving(document.store(), root, &candidates),
            Some(children[4])
        );
    }

    #[test]
    fn nothing_surviving_reports_nothing() {
        let (document, root, children) = row(3);
        crate::node::links::unlink(document.store(), children[2]);
        assert_eq!(
            earliest_surviving(document.store(), root, &[children[2]]),
            None
        );
        assert_eq!(earliest_surviving(document.store(), root, &[]), None);
    }

    #[test]
    fn a_position_recorded_before_a_later_insertion_is_not_believed() {
        let (mut document, root, children) = row(3);
        // Reading a position now numbers the list; the insertion below invalidates the numbering,
        // and the answer has to come from the new one.
        assert_eq!(document.store().ordinal_of(children[0]), 0);

        let fresh = document.detached(NodeKind::Element, ElementName::new("row"));
        crate::node::links::link_before(document.store(), root, fresh, Some(children[0]));
        assert_eq!(
            earliest_surviving(document.store(), root, &[children[0], fresh]),
            Some(fresh)
        );
        assert_eq!(document.store().ordinal_of(children[0]), 1);
    }
}
