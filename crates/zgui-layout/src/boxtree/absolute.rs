//! Moving an out-of-flow box onto the box that actually positions it.
//!
//! The layout algorithms resolve an absolutely positioned child against its *immediate* parent, and
//! CSS resolves it against the nearest positioned ancestor. Those are the same box only by
//! accident. So an out-of-flow box is re-parented while the tree is built: it is kept out of the
//! layout child list of the box it was written inside, and appended to the layout child list of the
//! box that positions it. When no ancestor is positioned, that box is the root's, which is the
//! initial containing block. Painting walks the layout child list, which is where CSS paints a
//! positioned box. Its entry in the *paint* child list stays where it was written, because
//! accessible geometry follows the document. A [`Declared`] record keeps where it was written, so
//! that painting can order it among its new siblings in tree order. A box that arrives later in a
//! containing block that is already built goes in at its place in post-order, see
//! [`attach_in_tree_order`].

use zgui_dom::side::BoxKey;
use zgui_dom::{Document, NodeIndex};

use crate::node::kind::FormattingContext;
use crate::tree::store::LayoutStore;

/// One out-of-flow box and the box that positions it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reparented {
    /// The out-of-flow box.
    pub key: BoxKey,
    /// The nearest positioned ancestor's box, which is its containing block.
    pub containing_block: BoxKey,
}

/// Where an out-of-flow box was written, recorded when it is re-parented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Declared {
    /// The box it was written inside.
    pub parent: BoxKey,
    /// The nearest in-flow box written before it inside `parent`, if any.
    ///
    /// The box paints after this sibling and before the next in-flow one. The key can go stale when
    /// a patch removes the sibling; a reader then searches the paint child list instead.
    pub after: Option<BoxKey>,
}

/// Whether a box establishes the containing block an out-of-flow descendant is positioned against.
///
/// Only `position` is consulted. A transform, a filter or a `will-change` also establishes one for
/// a fixed descendant, and each of those is a property of painting rather than of the box tree.
pub fn establishes_containing_block(positioned: bool, fc: FormattingContext) -> bool {
    positioned && fc != FormattingContext::None
}

/// Appends every re-parented box to the layout child list of the box that positions it.
///
/// Run once, after the whole tree is built, because a box's containing block is an ancestor and an
/// ancestor's child list is not final until its subtree is.
pub fn attach_all(store: &mut LayoutStore, reparented: &[Reparented]) {
    for item in reparented {
        let container_fc = store.node(item.containing_block).fc;
        if let Some(node) = store.get_mut(item.key) {
            node.parent = Some(item.containing_block);
            node.parent_fc = container_fc;
        }
        if let Some(container) = store.get_mut(item.containing_block) {
            container.children.push(item.key);
        }
    }
}

/// Puts one out-of-flow box into the layout child list of a containing block that is already built.
///
/// A build appends each re-parented box when its own subtree is complete, so the out-of-flow boxes
/// of one containing block follow the post-order of the elements that generated them, after every
/// in-flow child. A box that arrives later goes after the last out-of-flow box whose element comes
/// before its own in that order. Boxes of equal tree rank paint in list order, so this keeps their
/// painting order the order a build gives.
///
/// The list is searched from its end, because a box that arrives is most often the last one written.
/// Each step of the search walks siblings in the document, so an arrival early in a long run of
/// out-of-flow boxes costs about the length of that run times their distance in the document.
pub fn attach_in_tree_order(store: &mut LayoutStore, document: &Document, item: Reparented) {
    let source = |store: &LayoutStore, key: BoxKey| {
        store
            .get(key)
            .and_then(|node| node.source)
            .and_then(|source| document.store().index_of(source))
    };
    let arriving = source(store, item.key).map(|node| (node, ancestry(document, node)));
    let at = {
        let list = &store.node(item.containing_block).children;
        let mut at = 0;
        for (index, &held) in list.iter().enumerate().rev() {
            if store.declared(held).is_none() {
                at = index + 1;
                break;
            }
            if let Some((arriving, above_arriving)) = &arriving
                && let Some(held) = source(store, held)
                && post_order_before(document, held, *arriving, above_arriving)
            {
                at = index + 1;
                break;
            }
        }
        at
    };
    let container_fc = store.node(item.containing_block).fc;
    if let Some(node) = store.get_mut(item.key) {
        node.parent = Some(item.containing_block);
        node.parent_fc = container_fc;
    }
    if let Some(container) = store.get_mut(item.containing_block) {
        container.children.insert(at, item.key);
    }
}

/// `node` and every node above it, from the top of its tree down.
fn ancestry(document: &Document, node: NodeIndex) -> Vec<NodeIndex> {
    let store = document.store();
    let mut chain = vec![node];
    let mut next = store.core(node).parent();
    while let Some(current) = next {
        chain.push(current);
        next = store.core(current).parent();
    }
    chain.reverse();
    chain
}

/// Whether `a` comes before `b` when the document is walked in post-order. `above_b` is the
/// [`ancestry`] of `b`.
///
/// A descendant comes before its ancestor, and otherwise the order is tree order. Siblings are
/// searched in both directions at once, so the cost follows the distance between the two.
fn post_order_before(
    document: &Document,
    a: NodeIndex,
    b: NodeIndex,
    above_b: &[NodeIndex],
) -> bool {
    if a == b {
        return false;
    }
    let store = document.store();
    if above_b.contains(&a) {
        return false;
    }
    let above_a = ancestry(document, a);
    if above_a.contains(&b) {
        return true;
    }
    let shared = above_a
        .iter()
        .zip(above_b)
        .take_while(|(x, y)| x == y)
        .count();
    if shared == 0 {
        // The two nodes are in no common tree, so there is no order between them.
        return false;
    }
    let (Some(&from), Some(&to)) = (above_a.get(shared), above_b.get(shared)) else {
        return false;
    };
    let (mut later, mut earlier) = (Some(from), Some(from));
    while later.is_some() || earlier.is_some() {
        later = later.and_then(|node| store.core(node).next_sibling());
        earlier = earlier.and_then(|node| store.core(node).prev_sibling());
        if later == Some(to) {
            return true;
        }
        if earlier == Some(to) {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;
    use zgui_css::StyleDraft;
    use zgui_dom::side::BoxKey;

    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::tree::store::LayoutStore;

    use super::{Reparented, attach_all, establishes_containing_block};

    fn container(store: &mut LayoutStore) -> BoxKey {
        store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            BoxKind::Element,
            FormattingContext::Block,
        ))
    }

    #[test]
    fn a_static_box_positions_nothing_and_a_positioned_one_does() {
        assert!(!establishes_containing_block(
            false,
            FormattingContext::Block
        ));
        assert!(establishes_containing_block(true, FormattingContext::Block));
        // A box that generates no geometry positions nothing, whatever its `position` says.
        assert!(!establishes_containing_block(true, FormattingContext::None));
    }

    #[test]
    fn a_re_parented_box_becomes_a_layout_child_of_its_containing_block() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let ancestor = container(&mut store);
        let parent = container(&mut store);
        let floating = container(&mut store);
        store.get_mut(parent).expect("live").parent = Some(ancestor);
        attach_all(
            &mut store,
            &[Reparented {
                key: floating,
                containing_block: ancestor,
            }],
        );
        assert_eq!(store.node(ancestor).children, vec![floating]);
        assert_eq!(store.node(floating).parent, Some(ancestor));
        assert!(store.node(parent).children.is_empty());
    }
}
