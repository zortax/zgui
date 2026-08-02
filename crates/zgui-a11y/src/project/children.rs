//! Which nodes hang below which, in the tree an assistive technology walks.
//!
//! The document's own child order is the answer, with the nodes that have no presence left out:
//! a marker holds a place for content that comes and goes and announces nothing, and a text node
//! holding no characters says nothing either. Leaving them in would put identifiers in a parent's
//! child list that resolve to nodes a consumer then has to be told to ignore.

use accesskit::NodeId;
use zgui_dom::NodeKey;

use crate::id::to_a11y;
use crate::world::World;

/// The identifiers of `node`'s children, in document order.
pub fn of(world: &World<'_>, node: NodeKey) -> Vec<NodeId> {
    let store = world.document.store();
    let Some(index) = store.index_of(node) else {
        return Vec::new();
    };
    let mut children = Vec::new();
    let mut next = store.core(index).first_child();
    while let Some(child) = next {
        let key = store.key_of(child);
        if world.is_projected(key) {
            children.push(to_a11y(key));
        }
        next = store.core(child).next_sibling();
    }
    children
}

/// The parent `node` hangs below in the projected tree, if it has one.
///
/// Used to widen a rebuild: a node whose projection changed may have changed its parent's child
/// list, and accesskit takes a child list only from the parent.
pub fn parent_of(world: &World<'_>, node: NodeKey) -> Option<NodeKey> {
    let store = world.document.store();
    let index = store.index_of(node)?;
    let parent = store.core(index).parent()?;
    Some(store.key_of(parent))
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeKind};
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;

    use super::of;
    use crate::id::to_a11y;
    use crate::world::World;

    #[test]
    fn a_child_list_holds_the_nodes_a_consumer_can_resolve_and_no_others() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let first = document.append(root, NodeKind::Element, ElementName::new("box"));
        document.append(root, NodeKind::Marker, ElementName::new("#marker"));
        let text = document.append(root, NodeKind::Text, ElementName::new("#text"));
        let last = document.append(root, NodeKind::Element, ElementName::new("box"));
        document
            .edit(&EverythingMatters, |edit| edit.set_text(text, "hello"))
            .expect("not poisoned");

        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        };
        let store = document.store();
        assert_eq!(
            of(&world, store.key_of(root)),
            vec![to_a11y(store.key_of(first)), to_a11y(store.key_of(last))],
            "a marker holds a place and says nothing, and text is its element's name rather than \
             a node of its own"
        );
    }
}
