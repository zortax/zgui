//! Moving an out-of-flow box onto the box that actually positions it.
//!
//! The layout algorithms resolve an absolutely positioned child against its *immediate* parent, and
//! CSS resolves it against the nearest positioned ancestor. Those are the same box only by
//! accident. So an out-of-flow box is re-parented while the tree is built: it is kept out of the
//! layout child list of the box it was written inside, and appended to the layout child list of the
//! box that positions it. Its entry in the *paint* child list stays where it was written, because
//! painting order and accessible geometry follow the document.

use zgui_dom::side::BoxKey;

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
