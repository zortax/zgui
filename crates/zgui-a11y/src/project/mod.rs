//! One document node, turned into one accessibility node.
//!
//! A projection is a pure function of the frame: the same document, geometry and focus produce the
//! same node every time, which is what makes comparing this frame's node with the last one a valid
//! test for "has anything an assistive technology can see changed". Nothing here is cached and
//! nothing here is incremental; deciding *which* nodes to project is [`crate::build`]'s job.
//!
//! | Module | What it contributes |
//! |---|---|
//! | [`semantics`] | what the element declared about itself |
//! | [`relations`] | which other nodes it names, filtered to the ones that exist |
//! | [`geometry`] | where it is, and whether it clips what is inside it |
//! | [`name`] | the name an element that declared none takes from its own text |
//! | [`children`] | which nodes hang below it |
//! | [`actions`] | what it can be asked to do |
//! | [`presence`] | whether it is on the screen at all |
//!
//! # A text node is not a node here
//!
//! The characters inside an element are that element's *name*, not a child of it. A document is
//! written `<label>{count}</label>`, and what a screen reader says when the number ticks over is
//! "the label now reads 1" — one node, the label. Exposing the text as a node of its own would
//! make it two, would put a node in the tree that no relation can usefully point at, and would
//! disagree with the invalidation key this projection is scheduled by, which hashes exactly an
//! element's own text children.

pub mod actions;
pub mod children;
pub mod geometry;
pub mod name;
pub mod presence;
pub mod relations;
pub mod semantics;

use accesskit::{Node, Role};
use zgui_dom::{NodeKey, NodeKind};

use crate::world::World;

/// The accessibility node `node` projects to, or nothing when it is not in the projected tree.
///
/// "In the projected tree" is the whole of the test: a node that has been taken out of the document
/// is still in the arena until the frame ends, and projecting one would put a node in an update
/// that hangs below nothing — which the consumer rejects outright.
pub fn node(world: &World<'_>, node: NodeKey) -> Option<Node> {
    if !world.is_projected(node) {
        return None;
    }
    let store = world.document.store();
    let index = store.index_of(node)?;
    match store.core(index).kind() {
        NodeKind::Marker | NodeKind::Text => None,
        NodeKind::Document => Some(root(world)),
        NodeKind::Element => Some(element(world, node)),
    }
}

/// The node everything else hangs below.
///
/// It is the one node carrying a transform, and that transform is the display scale: every other
/// node's rectangle is in CSS pixels, so a window moving to a display of a different scale rewrites
/// one node rather than all of them.
fn root(world: &World<'_>) -> Node {
    let mut projected = Node::new(Role::Window);
    projected.set_transform(geometry::root_transform(world.scale));
    projected.set_children(children::of(world, world.root()));
    projected
}

/// An ordinary element.
fn element(world: &World<'_>, node: NodeKey) -> Node {
    let declared = world.semantics(node).cloned().unwrap_or_default();
    let mut projected = Node::new(declared.role);
    semantics::apply(&declared, &mut projected);
    // After the declaration and before everything else, because an element that generates no box is
    // hidden whatever it declared: a component that sets `display: none` on its own content has
    // said so in CSS, and the accessibility tree is not the place to disagree with it.
    if presence::is_absent(world, node) {
        projected.set_hidden();
    }
    name::apply(world, node, &declared, &mut projected);
    relations::apply(world, &declared.relations, &mut projected);
    geometry::apply(world, node, &mut projected);
    actions::apply(world, node, &declared, &mut projected);
    projected.set_children(children::of(world, node));
    projected
}

#[cfg(test)]
mod tests {
    use accesskit::{Node, Role};
    use zgui_dom::{Document, EverythingMatters, NodeKind};
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;

    use super::node;
    use crate::world::World;

    #[test]
    fn a_marker_projects_to_nothing_at_all() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let marker = document.append(root, NodeKind::Marker, ElementName::new("#marker"));
        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        };
        assert!(node(&world, document.store().key_of(marker)).is_none());
    }

    #[test]
    fn an_element_that_declared_nothing_is_a_box_a_consumer_drops() {
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
        let projected = node(&world, document.store().key_of(root)).expect("an element projects");
        assert_eq!(projected.role(), Role::GenericContainer);
    }

    #[test]
    fn the_same_document_projects_the_same_node_twice() {
        // The whole diff rests on this: a projection that varied would emit every node on every
        // frame while reporting that each one had changed.
        let document = Document::new();
        let button = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let button = edit.create_element(ElementName::new("control"));
                edit.insert_before(root, button, None);
                edit.set_semantics(
                    button,
                    Some(zgui_vocab::A11y::new(Role::Button).label("Save").into()),
                );
                button
            })
            .expect("not poisoned");
        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        };
        let key = document.store().key_of(button);
        let first: Option<Node> = node(&world, key);
        let second: Option<Node> = node(&world, key);
        assert_eq!(first, second);
    }
}
