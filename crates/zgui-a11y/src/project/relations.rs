//! The relations one node declares to others, written onto an accessibility node.
//!
//! Relations are the part of a node's meaning that is not about the node itself, and several whole
//! categories of control cannot be described without them: a field named by a label that is not its
//! ancestor, a tab that controls a panel elsewhere in the tree, a trigger that owns a pop-up the
//! framework had to move to the top of the window in order to draw it.
//!
//! # Every identifier is checked before it is written
//!
//! A consumer resolves a relation without checking it, so an identifier naming a node that is not
//! in the tree is not a missing feature but a crash in somebody else's thread. Every target is
//! therefore filtered through [`World::is_projected`], once, here — the one place that has both the
//! declaration and the tree in hand.
//!
//! [`World::is_projected`]: crate::World::is_projected

use accesskit::Node;
use zgui_vocab::Relations;

use crate::id::to_document;
use crate::world::World;

/// Writes `relations` onto `into`, dropping every target that is not in the projected tree.
pub fn apply(world: &World<'_>, relations: &Relations, into: &mut Node) {
    let resolve = |targets: &[accesskit::NodeId]| -> Vec<accesskit::NodeId> {
        targets
            .iter()
            .copied()
            .filter(|target| exists(world, *target))
            .collect()
    };

    let labelled_by = resolve(&relations.labelled_by);
    if !labelled_by.is_empty() {
        into.set_labelled_by(labelled_by);
    }
    let described_by = resolve(&relations.described_by);
    if !described_by.is_empty() {
        into.set_described_by(described_by);
    }
    let controls = resolve(&relations.controls);
    if !controls.is_empty() {
        into.set_controls(controls);
    }
    let owns = resolve(&relations.owns);
    if !owns.is_empty() {
        into.set_owns(owns);
    }
    let radio_group = resolve(&relations.radio_group);
    if !radio_group.is_empty() {
        into.set_radio_group(radio_group);
    }

    if let Some(target) = relations.active_descendant.filter(|id| exists(world, *id)) {
        into.set_active_descendant(target);
    }
    if let Some(target) = relations.popup_for.filter(|id| exists(world, *id)) {
        into.set_popup_for(target);
    }
    if let Some(target) = relations.error_message.filter(|id| exists(world, *id)) {
        into.set_error_message(target);
    }
}

/// Whether `target` names a node the projected tree actually holds.
fn exists(world: &World<'_>, target: accesskit::NodeId) -> bool {
    to_document(target).is_some_and(|key| world.is_projected(key))
}

/// Every identifier `node` relates to, in the order the relations were written.
///
/// This is what an integrity check walks: a relation is only ever wrong in one way, and it is the
/// same way for all eight of them.
pub fn targets_of(node: &Node) -> Vec<accesskit::NodeId> {
    let mut targets = Vec::new();
    for list in [
        node.labelled_by(),
        node.described_by(),
        node.controls(),
        node.owns(),
        node.radio_group(),
    ] {
        targets.extend_from_slice(list);
    }
    targets.extend(node.active_descendant());
    targets.extend(node.popup_for());
    targets.extend(node.error_message());
    targets
}

#[cfg(test)]
mod tests {
    use accesskit::{Node, NodeId, Role};
    use zgui_dom::{Document, EverythingMatters, NodeKind};
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;
    use zgui_vocab::Relations;

    use super::{apply, targets_of};
    use crate::id::to_a11y;
    use crate::world::World;

    #[test]
    fn a_target_that_left_the_document_is_dropped_rather_than_written() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let label = document.append(root, NodeKind::Element, ElementName::new("label"));
        let alive = to_a11y(document.store().key_of(label));

        document
            .edit(&EverythingMatters, |edit| edit.remove(label))
            .expect("not poisoned");
        zgui_dom::arena::end_frame(&mut document);

        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        };
        let relations = Relations {
            labelled_by: vec![alive],
            popup_for: Some(NodeId(0xdead_beef)),
            ..Relations::default()
        };

        let mut node = Node::new(Role::TextInput);
        apply(&world, &relations, &mut node);
        assert!(
            targets_of(&node).is_empty(),
            "a dangling identifier panics the consumer's own thread, so it must never be written"
        );
    }

    #[test]
    fn a_live_target_survives_the_filter() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let label = document.append(root, NodeKind::Element, ElementName::new("label"));
        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        };
        let target = to_a11y(document.store().key_of(label));

        let mut node = Node::new(Role::TextInput);
        apply(
            &world,
            &Relations {
                labelled_by: vec![target],
                described_by: vec![target],
                controls: vec![target],
                owns: vec![target],
                radio_group: vec![target],
                active_descendant: Some(target),
                popup_for: Some(target),
                error_message: Some(target),
            },
            &mut node,
        );
        assert_eq!(
            targets_of(&node).len(),
            8,
            "every relation shape has to reach the node, or a whole ARIA pattern is unreachable"
        );
    }
}
