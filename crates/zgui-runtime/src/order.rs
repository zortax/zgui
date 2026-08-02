//! Tree order between two nodes of one document.
//!
//! Tree order is the order a depth-first walk reaches nodes in: what a reader meets first, what a
//! menu's down-arrow moves to next, what a set of items registered from anywhere has to be sorted
//! back into. Registration order is not it — a keyed list rebuilds the rows whose keys moved, so
//! the order its items announced themselves in reflects the last reshuffle rather than the tree.

use zgui_dom::{DocumentStore, NodeKey};

/// Whether `first` comes before `second` in tree order.
///
/// A node never precedes itself, and a node that is not in the store precedes nothing.
///
/// The walk is up rather than down: both ancestor chains are collected to the root and compared,
/// which costs the two depths rather than the size of the subtree between them. Where the chains
/// diverge, the two diverging siblings are compared by walking the shared parent's child list — so
/// the whole thing is bounded by depth plus the width of one node's children.
pub(crate) fn precedes(store: &DocumentStore, first: NodeKey, second: NodeKey) -> bool {
    if first == second {
        return false;
    }
    let (Some(left), Some(right)) = (chain_to_root(store, first), chain_to_root(store, second))
    else {
        return false;
    };
    // Two nodes in two trees — one of them detached — have no order between them.
    if left.first() != right.first() {
        return false;
    }

    let shared = left
        .iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count();
    match (left.get(shared), right.get(shared)) {
        // They diverge below a common ancestor: whichever sibling comes first in the child list
        // takes its whole subtree with it.
        (Some(left), Some(right)) => sibling_precedes(store, *left, *right),
        // `first` is an ancestor of `second`, and an ancestor is reached first.
        (None, Some(_)) => true,
        // `second` is an ancestor of `first`.
        (Some(_), None) => false,
        (None, None) => false,
    }
}

/// The chain from the document's root down to `node`, root first.
fn chain_to_root(store: &DocumentStore, node: NodeKey) -> Option<Vec<NodeKey>> {
    let mut index = store.index_of(node)?;
    let mut chain = Vec::new();
    loop {
        let record = store.core(index);
        chain.push(record.key());
        match record.parent() {
            Some(parent) => index = parent,
            None => break,
        }
    }
    chain.reverse();
    Some(chain)
}

/// Whether `first` comes before `second` among the children of their shared parent.
fn sibling_precedes(store: &DocumentStore, first: NodeKey, second: NodeKey) -> bool {
    let Some(index) = store.index_of(first) else {
        return false;
    };
    let mut cursor = store.core(index).next_sibling();
    while let Some(next) = cursor {
        let record = store.core(next);
        if record.key() == second {
            return true;
        }
        cursor = record.next_sibling();
    }
    false
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeKey};
    use zgui_interned::ElementName;

    use super::precedes;

    /// A document shaped `root > (a > (a1, a2), b)`, and the key of every node in it.
    fn tree() -> (Document, [NodeKey; 5]) {
        let document = Document::new();
        let indices = document
            .edit(&EverythingMatters, |edit| {
                let name = ElementName::new("box");
                let root = edit.create_element(name);
                edit.insert_before(document.document_index(), root, None);
                let a = edit.create_element(name);
                edit.insert_before(root, a, None);
                let a1 = edit.create_element(name);
                edit.insert_before(a, a1, None);
                let a2 = edit.create_element(name);
                edit.insert_before(a, a2, None);
                let b = edit.create_element(name);
                edit.insert_before(root, b, None);
                [root, a, a1, a2, b]
            })
            .expect("not poisoned");
        let keys = {
            let store = document.store();
            indices.map(|index| store.key_of(index))
        };
        (document, keys)
    }

    #[test]
    fn tree_order_is_the_order_a_depth_first_walk_reaches_nodes_in() {
        let (document, order) = tree();
        let store = document.store();
        for (before, node) in order.iter().enumerate() {
            for after in &order[before + 1..] {
                assert!(
                    precedes(store, *node, *after),
                    "{node:?} should precede {after:?}"
                );
                assert!(
                    !precedes(store, *after, *node),
                    "and {after:?} should not precede {node:?}"
                );
            }
        }
    }

    #[test]
    fn an_ancestor_precedes_its_own_descendants() {
        let (document, [_root, a, a1, a2, _b]) = tree();
        let store = document.store();
        assert!(precedes(store, a, a1));
        assert!(precedes(store, a, a2));
        assert!(!precedes(store, a1, a));
    }

    #[test]
    fn a_later_subtree_follows_an_earlier_one_whole() {
        // The case registration order gets wrong: `b` is a sibling of `a`, so it follows
        // everything under `a` and not just `a` itself.
        let (document, [_root, _a, a1, a2, b]) = tree();
        let store = document.store();
        assert!(precedes(store, a1, b));
        assert!(precedes(store, a2, b));
        assert!(!precedes(store, b, a1));
    }

    #[test]
    fn a_node_does_not_precede_itself() {
        let (document, [_root, a, ..]) = tree();
        assert!(!precedes(document.store(), a, a));
    }
}
