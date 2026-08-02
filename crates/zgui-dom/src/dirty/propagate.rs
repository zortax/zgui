//! Recording that a node owes work, and that its ancestors must descend to find it.
//!
//! Marking is two writes and a walk. The node's own invalidation word gains the obligations; every
//! ancestor's *subtree* half gains them too, so that a phase walk starting at the document root can
//! skip any subtree in one word; and every ancestor's dirty-child record learns which child to
//! descend into, so that a parent with ten thousand children and one marked child probes one word
//! rather than ten thousand.
//!
//! # Why the walk stops early, and why one write happens before it stops
//!
//! The walk returns at the first ancestor whose subtree union already contains everything being
//! added, because everything above that ancestor already contains it too. That turns marking `n`
//! nodes into `O(n + depth)` steps rather than `O(n · depth)`.
//!
//! **The marked node itself is tested on its *own* bits and not on its subtree union**, and the two
//! are not the same test. A node's own bits are set by nothing but a mark, so finding them already
//! set means an earlier mark of this node already walked the ancestors — which is what makes the
//! early return safe. Its subtree union is also raised by the style engine's own descent flag, on
//! elements the marked set never leads to: a sibling whose descendant an invalidation reached. Such
//! a flag outlives the walk that retires the marked set, because that walk never visits the node
//! carrying it — and a later mark of that node, tested against its subtree union, would return
//! before telling a single ancestor. The result is an element that is never traversed again and
//! keeps a stale style with nothing to notice it by.
//!
//! The dirty-child record is widened **before** that early return, and the order is load-bearing: an
//! ancestor that already owes these obligations *for a different child* must still learn about this
//! one, or the walk that eventually descends will skip past it. Children are recorded by identity
//! and never by position, because positions among element siblings are numbered lazily and a mark
//! can happen between a structural change and the renumbering that follows it.
//!
//! # Why this takes the store exclusively
//!
//! The invalidation word is an atomic and would be safe to write through a shared borrow, but the
//! dirty-child record is twenty bytes of plain data in a cell. Marking is a between-frames operation
//! — it never runs while a traversal is reading the tree — and taking the store exclusively is what
//! makes that true rather than merely intended.

use zgui_bits::Dirty;

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;

/// Records that `node` owes `bits`, and that every ancestor must descend into it.
///
/// # Panics
///
/// Panics if `node` names no live node of `store`.
pub fn mark(store: &mut DocumentStore, node: NodeIndex, bits: Dirty) {
    let doc: &DocumentStore = store;
    let already_owed = doc.core(node).dirty().own().contains(bits);
    doc.core(node).dirty().mark(bits);
    if already_owed {
        return;
    }
    propagate(store, node, bits);
}

/// Records that every ancestor of `node` must descend into it to find `bits`.
///
/// The obligation itself is not added to `node`: this is the half of [`mark`] that runs after the
/// node's own word has been decided, exported for the one caller that has a node whose obligations
/// were recorded while it was detached. Linking such a node into a document leaves its own word
/// untouched and every ancestor of its new parent ignorant of it, and [`mark`] cannot repair that —
/// it returns at the node's own word, which already owes the bits, before reaching an ancestor.
///
/// # Panics
///
/// Panics if `node` names no live node of `store`.
pub fn propagate(store: &mut DocumentStore, node: NodeIndex, bits: Dirty) {
    if bits.is_clean() {
        return;
    }
    let doc: &DocumentStore = store;
    let mut child = node;
    let mut current = doc.core(node).parent();
    while let Some(parent) = current {
        let record = doc.core(parent);
        record.dirty_children().widen(parent, child, doc);
        if !record.dirty().mark_subtree(bits) {
            return;
        }
        child = parent;
        current = record.parent();
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_interned::ElementName;

    use super::mark;
    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    /// A chain of `depth` elements under the document node, innermost last.
    fn chain(depth: usize) -> (Document, Vec<crate::id::node_key::NodeIndex>) {
        let mut document = Document::new();
        let mut parent = document.document_index();
        let mut nodes = Vec::new();
        for _ in 0..depth {
            parent = document.append(parent, NodeKind::Element, ElementName::new("box"));
            nodes.push(parent);
        }
        (document, nodes)
    }

    #[test]
    fn marking_a_leaf_reaches_every_ancestor_and_nothing_else() {
        let (mut document, nodes) = chain(4);
        let leaf = *nodes.last().expect("the chain has nodes");
        mark(document.store_mut(), leaf, Dirty::RESTYLE);

        for node in &nodes {
            assert!(
                document
                    .store()
                    .core(*node)
                    .dirty()
                    .subtree()
                    .contains(Dirty::RESTYLE),
                "every ancestor of the marked node has to know there is work below it"
            );
        }
        assert!(
            !document
                .store()
                .core(nodes[0])
                .dirty()
                .own()
                .contains(Dirty::RESTYLE),
            "an ancestor owes nothing itself; only its subtree does"
        );
    }

    #[test]
    fn a_second_mark_of_the_same_node_stops_at_the_node_itself() {
        let (mut document, nodes) = chain(3);
        let leaf = *nodes.last().expect("the chain has nodes");
        mark(document.store_mut(), leaf, Dirty::RESTYLE);
        // Clearing one ancestor's union and marking again must not restore it: the second mark
        // returns at the leaf's own cell, which already owes the bits. That is the property every
        // separately-retired descent flag falls foul of.
        document
            .store()
            .core(nodes[0])
            .dirty()
            .retire_phase(Dirty::RESTYLE, Dirty::empty());
        mark(document.store_mut(), leaf, Dirty::RESTYLE);
        assert!(
            !document
                .store()
                .core(nodes[0])
                .dirty()
                .subtree()
                .contains(Dirty::RESTYLE)
        );
    }

    #[test]
    fn a_node_carrying_only_the_engines_descent_flag_still_marks_its_ancestors() {
        let (mut document, nodes) = chain(3);
        let middle = nodes[1];
        // Exactly what the style engine's own descent flag does: it raises the subtree union of an
        // element it is about to descend into, and it does that on elements no mark ever led to.
        document.node(middle).note_style_work_below();

        mark(document.store_mut(), middle, Dirty::RESTYLE);

        assert!(
            document
                .store()
                .core(nodes[0])
                .dirty()
                .subtree()
                .contains(Dirty::RESTYLE),
            "a mark tested against the subtree union would have returned here, and this node would \
             never have been traversed again"
        );
        let marked: Vec<_> = document
            .store()
            .core(nodes[0])
            .dirty_children()
            .iter(document.store(), nodes[0])
            .collect();
        assert!(marked.contains(&middle));
    }

    #[test]
    fn an_ancestor_that_already_owes_the_bits_still_learns_about_a_second_child() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let first = document.append(root, NodeKind::Element, ElementName::new("item"));
        let second = document.append(root, NodeKind::Element, ElementName::new("item"));

        mark(document.store_mut(), first, Dirty::RESTYLE);
        mark(document.store_mut(), second, Dirty::RESTYLE);

        let marked: Vec<_> = document
            .store()
            .core(root)
            .dirty_children()
            .iter(document.store(), root)
            .collect();
        assert!(marked.contains(&first));
        assert!(
            marked.contains(&second),
            "the second child is the one an early return before the widening would lose"
        );
    }
}
