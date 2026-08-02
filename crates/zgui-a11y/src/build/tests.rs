//! What an update is allowed to contain, checked against a real document.

use accesskit::{NodeId, Role};
use zgui_dom::{Document, EverythingMatters, NodeIndex};
use zgui_interned::ElementName;
use zgui_layout::tree::store::LayoutStore;
use zgui_vocab::{A11y, EventKind, ListenerOptions};

use super::A11yBuilder;
use crate::id::to_a11y;
use crate::world::World;

/// A document with a button and a text child under a root, as a view would have built it.
fn counter() -> (Document, NodeIndex, NodeIndex) {
    let document = Document::new();
    let (button, text) = document
        .edit(&EverythingMatters, |edit| {
            let root = edit.create_element(ElementName::new("root"));
            edit.insert_before(document.document_index(), root, None);
            let button = edit.create_element(ElementName::new("control"));
            edit.insert_before(root, button, None);
            edit.add_listener(button, EventKind::Click, ListenerOptions::DEFAULT);
            edit.set_semantics(
                button,
                Some(A11y::new(Role::Button).label("Increment").into()),
            );
            let text = edit.create_text("0");
            edit.insert_before(root, text, None);
            (button, text)
        })
        .expect("a fresh document is not poisoned");
    (document, button, text)
}

/// A world over `document` with nothing laid out and nothing focused.
macro_rules! world {
    ($document:expr, $layout:expr) => {
        World {
            document: &$document,
            layout: &$layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        }
    };
}

#[test]
fn the_first_update_is_a_whole_tree_with_a_root() {
    let (mut document, _button, _text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);

    let update = builder.build(&world!(document, layout));
    assert_eq!(
        update.tree.as_ref().map(|tree| tree.root),
        Some(to_a11y(document.store().key_of(document.document_index()))),
        "a consumer holding nothing needs the tree data or it has no root to hang anything on"
    );
    assert_eq!(
        update.nodes.len(),
        3,
        "the document node, the root, and the button"
    );
    assert!(crate::dangling(&update, builder.retained()).is_empty());
}

#[test]
fn a_text_edit_changes_exactly_one_accessibility_node() {
    let (mut document, _button, text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    builder.build(&world!(document, layout));

    // What clicking the increment button does to the document, through the same edit path.
    document
        .edit(&EverythingMatters, |edit| edit.set_text(text, "1"))
        .expect("not poisoned");
    builder.collect(&mut document);
    let update = builder.build(&world!(document, layout));

    assert_eq!(
        update.nodes.len(),
        1,
        "the element the text names is the one node that changed, and nothing else may be sent \
         with it: {}",
        crate::dump(&update)
    );
    let root = document.root_index().expect("the document has a root");
    assert_eq!(update.nodes[0].0, to_a11y(document.store().key_of(root)));
    let _ = text;
    assert!(crate::dangling(&update, builder.retained()).is_empty());
}

#[test]
fn an_unchanged_frame_sends_no_nodes_at_all() {
    let (mut document, _button, _text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    builder.build(&world!(document, layout));

    builder.collect(&mut document);
    let update = builder.build(&world!(document, layout));
    assert!(update.nodes.is_empty());
}

#[test]
fn an_inserted_node_arrives_with_the_parent_that_now_lists_it() {
    let (mut document, _button, _text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    builder.build(&world!(document, layout));

    let root = document.root_index().expect("the document has a root");
    let added = document
        .edit(&EverythingMatters, |edit| {
            let added = edit.create_element(ElementName::new("control"));
            edit.insert_before(root, added, None);
            edit.set_semantics(added, Some(A11y::new(Role::Button).label("Reset").into()));
            added
        })
        .expect("not poisoned");
    builder.collect(&mut document);
    let update = builder.build(&world!(document, layout));

    let ids: Vec<NodeId> = update.nodes.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&to_a11y(document.store().key_of(added))));
    assert!(
        ids.contains(&to_a11y(document.store().key_of(root))),
        "accesskit takes a child list only from the parent, so a child sent without its parent \
         is a node the consumer rejects: {}",
        crate::dump(&update)
    );
    assert!(crate::dangling(&update, builder.retained()).is_empty());
}

#[test]
fn a_removed_node_leaves_no_identifier_behind_it() {
    let (mut document, button, _text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    builder.build(&world!(document, layout));

    document
        .edit(&EverythingMatters, |edit| edit.remove(button))
        .expect("not poisoned");
    builder.collect(&mut document);
    let update = builder.build(&world!(document, layout));
    zgui_dom::arena::end_frame(&mut document);

    assert!(
        !update.nodes.is_empty(),
        "removing a node has to re-send the parent, or the consumer keeps the child for ever"
    );
    assert!(
        crate::dangling(&update, builder.retained()).is_empty(),
        "{}",
        crate::dump(&update)
    );
}

#[test]
fn a_relation_survives_into_the_update_and_resolves() {
    let document = Document::new();
    let (field, label) = document
        .edit(&EverythingMatters, |edit| {
            let root = edit.create_element(ElementName::new("root"));
            edit.insert_before(document.document_index(), root, None);
            let label = edit.create_element(ElementName::new("label"));
            edit.insert_before(root, label, None);
            let field = edit.create_element(ElementName::new("control"));
            edit.insert_before(root, field, None);
            (field, label)
        })
        .expect("not poisoned");
    let label_id = to_a11y(document.store().key_of(label));
    document
        .edit(&EverythingMatters, |edit| {
            edit.set_semantics(
                field,
                Some(A11y::new(Role::TextInput).labelled_by(label_id).into()),
            );
        })
        .expect("not poisoned");

    let mut document = document;
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    let update = builder.build(&world!(document, layout));

    let text = crate::dump(&update);
    assert!(
        text.contains("labelled_by="),
        "the relation the whole label pattern rests on is absent: {text}"
    );
    assert!(crate::dangling(&update, builder.retained()).is_empty());
}

#[test]
fn a_node_that_named_a_removed_one_is_re_sent_without_the_relation() {
    // The failure no single update can show. The field is untouched by the change — its own
    // declaration, its own text and its own children are all exactly what they were — so nothing
    // marks it, and only the departure of what it names says it is now wrong. A consumer left
    // holding the relation resolves it with an unchecked lookup the moment a screen reader asks
    // the field for its name.
    let document = Document::new();
    let (field, hint) = document
        .edit(&EverythingMatters, |edit| {
            let root = edit.create_element(ElementName::new("root"));
            edit.insert_before(document.document_index(), root, None);
            let field = edit.create_element(ElementName::new("control"));
            edit.insert_before(root, field, None);
            let holder = edit.create_element(ElementName::new("box"));
            edit.insert_before(root, holder, None);
            let hint = edit.create_element(ElementName::new("label"));
            edit.insert_before(holder, hint, None);
            (field, hint)
        })
        .expect("not poisoned");
    let hint_id = to_a11y(document.store().key_of(hint));
    let holder = document.store().core(hint).parent().expect("a parent");
    document
        .edit(&EverythingMatters, |edit| {
            edit.set_semantics(
                field,
                Some(A11y::new(Role::TextInput).labelled_by(hint_id).into()),
            );
        })
        .expect("not poisoned");

    let mut document = document;
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    let first = builder.build(&world!(document, layout));
    assert!(
        crate::dump(&first).contains("labelled_by="),
        "the relation was never written, so taking its target away proves nothing: {}",
        crate::dump(&first)
    );

    // The whole holder goes, so the named node is a *descendant* of what was removed: the only
    // node the removal marks is the holder's parent.
    document
        .edit(&EverythingMatters, |edit| edit.remove(holder))
        .expect("not poisoned");
    builder.collect(&mut document);
    let update = builder.build(&world!(document, layout));

    let field_id = to_a11y(document.store().key_of(field));
    let resent = update
        .nodes
        .iter()
        .find(|(id, _)| *id == field_id)
        .map(|(_, node)| node);
    let resent = resent.unwrap_or_else(|| {
        panic!(
            "the field still names a node the consumer has dropped: {}",
            crate::dump(&update)
        )
    });
    assert!(
        crate::project::relations::targets_of(resent).is_empty(),
        "{}",
        crate::dump(&update)
    );
}

#[test]
fn focus_is_reported_on_every_update_and_never_dangles() {
    let (mut document, button, _text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);

    let focused = document.store().key_of(button);
    let world = World {
        document: &document,
        layout: &layout,
        placements: &zgui_scene::Placements::EMPTY,
        scale: 1.0,
        focus: Some(focused),
    };
    let update = builder.build(&world);
    assert_eq!(update.focus, to_a11y(focused));

    // And a focus report for a node that is not in the tree falls back to the root rather than
    // naming something the consumer would resolve with an unchecked lookup.
    let stale = World {
        document: &document,
        layout: &layout,
        placements: &zgui_scene::Placements::EMPTY,
        scale: 1.0,
        focus: zgui_dom::NodeKey::from_u64(0xdead_beef),
    };
    let update = builder.focus_update(&stale);
    assert_eq!(
        update.focus,
        to_a11y(document.store().key_of(document.document_index()))
    );
}

#[test]
fn forgetting_the_tree_makes_the_next_update_a_whole_one() {
    let (mut document, _button, _text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    let first = builder.build(&world!(document, layout));

    builder.forget();
    let again = builder.build(&world!(document, layout));
    assert_eq!(first.nodes.len(), again.nodes.len());
    assert!(again.tree.is_some());
}

#[test]
fn a_coordinate_system_that_moved_owes_a_rectangle_for_what_was_measured_through_it() {
    // The obligation this crate holds instead of the one the fragment pass raises. A node that was
    // carried somewhere else is reported by name because a walk touched it; a node whose
    // coordinate system was written to was not touched by anything, and the only record that it is
    // drawn elsewhere is what its space now resolves to. So the way back — from a name for a space
    // to what was published through it — is kept here, where it outlives any walk.
    let (mut document, button, _text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    builder.build(&world!(document, layout));
    assert!(!builder.is_owed(), "the tree has just been published");

    let mut tree = zgui_scene::SpatialTree::with_viewport();
    let owner =
        |raw| zgui_scene::PropertyOwner::new(raw).expect("a handle is never the empty word");
    let slid = |x| {
        zgui_scene::OwnSpace::of(
            Some(zgui_geom::Matrix4::translation(x, 0.0, 0.0)),
            None,
            false,
        )
    };
    let panel = tree.space_of(tree.viewport(), owner(2), slid(4.0));
    let elsewhere = tree.space_of(tree.viewport(), owner(3), slid(9.0));

    let key = document.store().key_of(button);
    builder.held.measured_through(key, core::iter::once(panel));

    builder.note_space_moved(elsewhere);
    assert!(
        !builder.is_owed(),
        "a coordinate system nothing was measured through owes nothing, or every animation in the \
         window would re-announce every control in it"
    );

    builder.note_space_moved(panel);
    assert!(
        builder.is_owed(),
        "the control's rectangle was measured through a space that now resolves to a different \
         matrix, so what the consumer is holding describes where the control used to be drawn"
    );
    let update = builder.build(&world!(document, layout));
    let _ = update;
}

#[test]
fn a_node_filed_under_a_space_it_has_left_is_not_answered_for_it() {
    // A box that has changed coordinate system has not moved within the one it was in — it has
    // left it. A filing that accumulated would keep answering for a matrix the node's rectangle no
    // longer depends on, which is an update sent on every frame of an animation happening
    // somewhere else in the window.
    let (mut document, button, _text) = counter();
    let layout = LayoutStore::new(document.store().document());
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);
    builder.build(&world!(document, layout));

    let mut tree = zgui_scene::SpatialTree::with_viewport();
    let owner =
        |raw| zgui_scene::PropertyOwner::new(raw).expect("a handle is never the empty word");
    let own = zgui_scene::OwnSpace::of(
        Some(zgui_geom::Matrix4::translation(4.0, 0.0, 0.0)),
        None,
        false,
    );
    let before = tree.space_of(tree.viewport(), owner(2), own);
    let after = tree.space_of(tree.viewport(), owner(3), own);

    let key = document.store().key_of(button);
    builder.held.measured_through(key, core::iter::once(before));
    builder.held.measured_through(key, core::iter::once(after));

    builder.note_space_moved(before);
    assert!(
        !builder.is_owed(),
        "the node is measured in the space it is in now, and in no other"
    );
    builder.note_space_moved(after);
    assert!(builder.is_owed());
}

#[test]
fn nothing_is_filed_for_a_consumer_that_is_holding_nothing() {
    // The whole cost of this on a machine with no assistive technology running. What a moved
    // coordinate system is looked up in is what the consumer holds, and that is empty until an
    // update has been built — so a window animating a transform sixty times a second does one
    // failed lookup per moved space per frame and no work at all.
    let (mut document, button, _text) = counter();
    let mut builder = A11yBuilder::new();
    builder.collect(&mut document);

    let mut tree = zgui_scene::SpatialTree::with_viewport();
    let owner = zgui_scene::PropertyOwner::new(2).expect("a handle is never the empty word");
    let own = zgui_scene::OwnSpace::of(
        Some(zgui_geom::Matrix4::translation(4.0, 0.0, 0.0)),
        None,
        false,
    );
    let panel = tree.space_of(tree.viewport(), owner, own);

    let key = document.store().key_of(button);
    builder.held.measured_through(key, core::iter::once(panel));
    assert_eq!(
        builder.held.measured_in(panel).count(),
        0,
        "a node the consumer is not holding has no rectangle to correct"
    );
}
