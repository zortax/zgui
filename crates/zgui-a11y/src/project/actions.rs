//! Which actions a node advertises, derived from what the document can actually do about them.
//!
//! An advertised action that a component then has to implement separately is the defect this whole
//! path exists to avoid: what is advertised here is exactly what [`crate::action`] routes back into
//! the document, and nothing else. A button is activatable because it has a click listener, not
//! because somebody remembered to say so.

use accesskit::{Action, Node};
use zgui_dom::NodeKey;
use zgui_vocab::{EventKind, Semantics};

use crate::world::World;

/// Adds every action `node` can actually be asked to perform.
pub fn apply(world: &World<'_>, node: NodeKey, semantics: &Semantics, into: &mut Node) {
    if listens_for(world, node, EventKind::Click) {
        into.add_action(Action::Click);
    }
    if zgui_input::focus::order::is_focusable(world.document.store(), Some(world.layout), node) {
        into.add_action(Action::Focus);
        if world.focus == Some(node) {
            into.add_action(Action::Blur);
        }
    }
    // A control that measures rather than names is stepped, and one that names is set. Both arrive
    // as an ordinary value change, which is the event the control's own handler already reads.
    if semantics.numeric.is_set() {
        into.add_action(Action::Increment);
        into.add_action(Action::Decrement);
    }
    if semantics.value.is_some() || listens_for(world, node, EventKind::Change) {
        into.add_action(Action::SetValue);
    }
}

/// Whether `node` has a listener registered for `kind`.
fn listens_for(world: &World<'_>, node: NodeKey, kind: EventKind) -> bool {
    world
        .document
        .store()
        .columns()
        .listeners
        .get(node)
        .is_some_and(|set| set.iter().any(|listener| listener.kind == kind))
}

#[cfg(test)]
mod tests {
    use accesskit::{Action, Node, Role};
    use zgui_dom::{Document, EverythingMatters};
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;
    use zgui_vocab::{EventKind, ListenerOptions, Semantics};

    use super::apply;
    use crate::world::World;

    #[test]
    fn only_a_node_with_a_click_listener_advertises_activation() {
        let document = Document::new();
        let (listening, silent) = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let listening = edit.create_element(ElementName::new("control"));
                edit.insert_before(root, listening, None);
                edit.add_listener(listening, EventKind::Click, ListenerOptions::DEFAULT);
                let silent = edit.create_element(ElementName::new("box"));
                edit.insert_before(root, silent, None);
                (listening, silent)
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
        let store = document.store();

        let mut activatable = Node::new(Role::Button);
        apply(
            &world,
            store.key_of(listening),
            &Semantics::default(),
            &mut activatable,
        );
        let mut inert = Node::new(Role::GenericContainer);
        apply(
            &world,
            store.key_of(silent),
            &Semantics::default(),
            &mut inert,
        );

        assert!(activatable.supports_action(Action::Click));
        assert!(
            !inert.supports_action(Action::Click),
            "an advertised action nothing can carry out is a control that does nothing when it \
             is activated"
        );
    }

    #[test]
    fn a_measured_control_advertises_both_steps() {
        let document = Document::new();
        let slider = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let slider = edit.create_element(ElementName::new("control"));
                edit.insert_before(root, slider, None);
                slider
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
        let semantics: Semantics = zgui_vocab::A11y::new(Role::Slider)
            .numeric_value(0.5)
            .numeric_range(0.0, 1.0)
            .into();

        let mut node = Node::new(Role::Slider);
        apply(
            &world,
            document.store().key_of(slider),
            &semantics,
            &mut node,
        );
        assert!(node.supports_action(Action::Increment));
        assert!(node.supports_action(Action::Decrement));
    }
}
