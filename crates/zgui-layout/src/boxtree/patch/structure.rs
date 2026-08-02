//! Taking one box out of a tree, and putting another in its place.
//!
//! A change to one element's `display`, `position`, `float` or `content` changes the boxes that
//! element generates and no others, so the tree is patched at that element rather than rebuilt from
//! the document root. What the patch owes the rest of the tree is the invalidation: every box from
//! the replacement up to the root has to have its layout thrown away, or the ancestors keep the
//! sizes they computed against the boxes that are gone.

use zgui_dom::side::BoxKey;

use crate::tree::dirty::mark_dirty;
use crate::tree::store::LayoutStore;

/// Removes one box and everything below it, and invalidates every ancestor's layout.
///
/// Returns how many boxes were removed.
pub fn detach(store: &mut LayoutStore, key: BoxKey) -> u32 {
    let Some(parent) = store.get(key).and_then(|node| node.parent) else {
        return remove_subtree(store, key);
    };
    if let Some(node) = store.get_mut(parent) {
        node.children.retain(|&child| child != key);
        node.paint_children.retain(|&child| child != key);
    }
    let removed = remove_subtree(store, key);
    mark_dirty(store, parent);
    removed
}

/// Removes a box and its descendants, leaving every other list alone.
fn remove_subtree(store: &mut LayoutStore, key: BoxKey) -> u32 {
    let mut removed = 0;
    let mut stack = vec![key];
    while let Some(current) = stack.pop() {
        let Some(node) = store.get(current) else {
            continue;
        };
        stack.extend(node.children.iter().copied());
        stack.extend(
            node.paint_children
                .iter()
                .copied()
                .filter(|child| !node.children.contains(child)),
        );
        if store.remove(current) {
            removed += 1;
        }
    }
    removed
}

/// Puts a freshly built box in place of `old` under the same parent, at the same index.
///
/// Returns whether there was an old box to replace.
pub fn replace(store: &mut LayoutStore, old: BoxKey, new: BoxKey) -> bool {
    let Some(parent) = store.get(old).and_then(|node| node.parent) else {
        return false;
    };
    let parent_fc = store.node(parent).fc;
    if let Some(node) = store.get_mut(parent) {
        for slot in node
            .children
            .iter_mut()
            .chain(node.paint_children.iter_mut())
        {
            if *slot == old {
                *slot = new;
            }
        }
    }
    if let Some(node) = store.get_mut(new) {
        node.parent = Some(parent);
        node.parent_fc = parent_fc;
    }
    remove_subtree(store, old);
    mark_dirty(store, parent);
    true
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;
    use zgui_css::StyleDraft;
    use zgui_dom::side::BoxKey;

    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::tree::store::LayoutStore;

    use super::{detach, replace};

    fn insert(store: &mut LayoutStore, parent: Option<BoxKey>) -> BoxKey {
        let key = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        if let Some(parent) = parent {
            store.get_mut(key).expect("live").parent = Some(parent);
            let node = store.get_mut(parent).expect("live");
            node.children.push(key);
            node.paint_children.push(key);
        }
        key
    }

    #[test]
    fn detaching_removes_the_subtree_and_unlinks_it_from_both_orders() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let root = insert(&mut store, None);
        let child = insert(&mut store, Some(root));
        let grandchild = insert(&mut store, Some(child));
        assert_eq!(detach(&mut store, child), 2);
        assert!(store.node(root).children.is_empty());
        assert!(store.node(root).paint_children.is_empty());
        store.recycle();
        assert!(!store.contains(child));
        assert!(!store.contains(grandchild));
    }

    #[test]
    fn replacing_keeps_the_position_in_both_orders() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let root = insert(&mut store, None);
        let first = insert(&mut store, Some(root));
        let second = insert(&mut store, Some(root));
        let fresh = insert(&mut store, None);
        assert!(replace(&mut store, first, fresh));
        assert_eq!(store.node(root).children, vec![fresh, second]);
        assert_eq!(store.node(root).paint_children, vec![fresh, second]);
        assert_eq!(store.node(fresh).parent, Some(root));
    }

    #[test]
    fn replacing_a_root_box_reports_that_there_was_no_parent_to_replace_it_under() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let root = insert(&mut store, None);
        let fresh = insert(&mut store, None);
        assert!(!replace(&mut store, root, fresh));
    }
}
