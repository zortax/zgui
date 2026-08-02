//! Whether a node is on the screen at all, and therefore whether it is announced.
//!
//! An element that generates no box generates nothing: `display: none` removes a subtree from the
//! rendering entirely, and CSS is explicit that it is removed from what an assistive technology
//! reads with it. A tree that announced it anyway would offer a screen-reader user a button that
//! is not on the screen, that no pointer can reach, and that does nothing when it is activated —
//! which is worse than an unlabelled control, because nothing about it sounds wrong.
//!
//! # Why the answer is not simply "generated no box"
//!
//! `display: contents` also generates no box, and it is *not* hidden: it puts its children in its
//! parent's place and stays in the accessibility tree itself. So a node counts as absent only when
//! nothing below it generated a box either, which is exactly the difference between the two.
//!
//! The descendant walk is bounded by the boxless region it is walking: it stops at the first box it
//! finds, so a `display: contents` wrapper costs one child and a hidden subtree costs itself.

use zgui_dom::{NodeKey, NodeKind};

use crate::world::World;

/// Whether `node` is absent from the rendering, and so from what is read aloud.
///
/// Always false before layout has run at all: a document with no boxes yet is one where every
/// answer would be "hidden", which is a tree that says nothing rather than a tree that is right.
pub fn is_absent(world: &World<'_>, node: NodeKey) -> bool {
    if world.layout.root().is_none() {
        return false;
    }
    !generates_anything(world, node)
}

/// Whether `node` or anything below it generated a box.
fn generates_anything(world: &World<'_>, node: NodeKey) -> bool {
    let store = world.document.store();
    let Some(index) = store.index_of(node) else {
        return false;
    };
    let mut stack = vec![index];
    while let Some(index) = stack.pop() {
        let key = store.key_of(index);
        if !world.layout.boxes_of(key).is_empty() {
            return true;
        }
        let mut child = store.core(index).first_child();
        while let Some(current) = child {
            if store.core(current).kind() != NodeKind::Marker {
                stack.push(current);
            }
            child = store.core(current).next_sibling();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;

    use super::is_absent;
    use crate::world::World;

    #[test]
    fn nothing_is_absent_before_layout_has_run() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        };
        assert!(
            !is_absent(&world, document.store().key_of(root)),
            "a document nothing has laid out yet is not a document where everything is hidden"
        );
    }
}
