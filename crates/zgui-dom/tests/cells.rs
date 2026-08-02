//! The single-threaded paths through the record, small enough to run under an interpreter.
//!
//! Nothing here starts a thread. The point is the record itself — that a handle stays valid while
//! the document keeps growing, that the back-pointer resolves, that every cell and every atomic
//! round-trips, and that the two child chains disagree in exactly the way they are supposed to.
//! Those are the paths an aliasing checker can actually reach, and this file is the one the
//! interpreter is pointed at.

use zgui_dom::node::atomics;
use zgui_dom::side::listeners::{Listener, ListenerId, ListenerSet};
use zgui_dom::{Dirty, Document, NodeFlags, NodeIndex, NodeKind};
use zgui_interned::{ClassName, ElementName, Ident};
use zgui_vocab::{EventKind, ListenerOptions, UiState};

/// A five-node document: a root, two element children with a text node between them, and a leaf.
fn small() -> (Document, [NodeIndex; 4]) {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let first = document.append(root, NodeKind::Element, ElementName::new("item"));
    document.append(root, NodeKind::Text, ElementName::new("#text"));
    let second = document.append(root, NodeKind::Element, ElementName::new("item"));
    let leaf = document.append(first, NodeKind::Element, ElementName::new("label"));
    document.set_classes(first, &[ClassName::new("item"), ClassName::new("card")]);
    document.set_classes(second, &[ClassName::new("item")]);
    (document, [root, first, second, leaf])
}

#[test]
fn a_handle_stays_valid_while_the_document_grows() {
    let (mut document, [root, first, _, _]) = small();
    let held = core::ptr::from_ref(document.node(first).record());
    for _ in 0..512 {
        document.append(root, NodeKind::Element, ElementName::new("item"));
    }
    assert_eq!(
        held,
        core::ptr::from_ref(document.node(first).record()),
        "the arena does not move what it has already handed out"
    );
}

#[test]
fn the_back_pointer_resolves_to_the_owning_store() {
    let (document, [root, first, _, _]) = small();
    let node = document.node(first);
    assert_eq!(node.store().len(), document.len());
    assert_eq!(node.record().parent(), Some(root));
    assert_eq!(node.store().key_of(first), node.key());
    assert_eq!(node.store().index_of(node.key()), Some(first));
}

#[test]
fn the_element_chain_and_the_plain_chain_differ_across_a_text_node() {
    let (document, [root, first, second, _]) = small();
    let store = document.store();

    assert_eq!(store.core(first).next_element(), Some(second));
    assert_eq!(store.core(second).prev_element(), Some(first));
    assert_eq!(store.core(root).first_element_child(), Some(first));

    let between = store
        .core(first)
        .next_sibling()
        .expect("the text node is next on the plain chain");
    assert_ne!(
        between, second,
        "the plain chain still has the text node in it"
    );
    assert_eq!(store.core(between).kind(), NodeKind::Text);
    assert_eq!(
        store.core(between).next_element(),
        None,
        "text is not on the element chain"
    );
    assert_eq!(store.core(root).child_count(), 3);
}

#[test]
fn selector_flags_accumulate_rather_than_replace() {
    use selectors::matching::ElementSelectorFlags;

    let (document, [_, first, _, _]) = small();
    let record = document.store().core(first);
    record.insert_selector_flags(ElementSelectorFlags::HAS_EMPTY_SELECTOR);
    assert!(
        record
            .selector_flags()
            .contains(ElementSelectorFlags::HAS_EMPTY_SELECTOR)
    );

    record.insert_selector_flags(ElementSelectorFlags::HAS_SLOW_SELECTOR);
    assert!(
        record.selector_flags().contains(
            ElementSelectorFlags::HAS_EMPTY_SELECTOR | ElementSelectorFlags::HAS_SLOW_SELECTOR
        ),
        "a second write accumulates"
    );
}

#[test]
fn the_bookkeeping_word_round_trips_one_bit_at_a_time() {
    let (document, [_, first, _, _]) = small();
    let record = document.store().core(first);
    assert!(!record.is_styled());

    for bit in [
        atomics::HAS_SNAPSHOT,
        atomics::SNAPSHOT_HANDLED,
        atomics::STYLED,
        atomics::ANIMATION_DIRTY_DESCENDANTS,
    ] {
        record.set_atomic(bit);
        assert!(record.has_atomic(bit));
        record.clear_atomic(bit);
        assert!(!record.has_atomic(bit));
    }

    record.set_atomic(atomics::STYLED);
    assert!(record.is_styled());
}

#[test]
fn the_descendants_question_is_answered_from_the_invalidation_word() {
    let (document, [root, first, _, _]) = small();
    let store = document.store();
    assert!(!store.core(root).has_dirty_descendants(Dirty::RESTYLE));

    store.core(first).dirty().mark(Dirty::RESTYLE);
    store.core(root).dirty().mark_subtree(Dirty::RESTYLE);
    assert!(store.core(root).has_dirty_descendants(Dirty::RESTYLE));
    assert!(
        !store.core(root).has_dirty_descendants(Dirty::REPAINT),
        "a different obligation is a different answer"
    );

    store
        .core(root)
        .dirty()
        .retire_phase(Dirty::RESTYLE, Dirty::empty());
    assert!(!store.core(root).has_dirty_descendants(Dirty::RESTYLE));
}

#[test]
fn the_post_order_counter_counts_down_to_zero() {
    let (document, [root, _, _, _]) = small();
    let record = document.store().core(root);
    record.store_children_to_process(3);
    assert_eq!(record.did_process_child(), 2);
    assert_eq!(record.did_process_child(), 1);
    assert_eq!(record.did_process_child(), 0);
}

#[test]
fn state_classes_and_identifiers_round_trip_through_their_cells() {
    let (mut document, [_, first, _, _]) = small();
    document.set_state(first, UiState::HOVER | UiState::FOCUS);
    document.set_id(first, Some(Ident::new("subject")));

    let store = document.store();
    assert_eq!(
        zgui_dom::node::element::state::from_engine(store.core(first).state()),
        UiState::HOVER | UiState::FOCUS
    );
    assert_eq!(
        store
            .classes_of(first)
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>(),
        vec!["item", "card"]
    );
    let id = store.core(first).id_attr().expect("an id was set");
    assert_eq!(
        store
            .idents()
            .resolve(id)
            .map(ToString::to_string)
            .as_deref(),
        Some("subject")
    );

    document.set_classes(first, &[ClassName::new("only")]);
    assert_eq!(document.store().classes_of(first).len(), 1);

    document.set_id(first, None);
    assert!(document.store().core(first).id_attr().is_none());
}

#[test]
fn the_flag_cell_round_trips() {
    let (mut document, [root, first, _, _]) = small();
    assert!(document.store().core(root).has_flags(NodeFlags::IS_ROOT));
    assert!(!document.store().core(first).has_flags(NodeFlags::IS_ROOT));

    document.set_flags(first, NodeFlags::IN_DOCUMENT | NodeFlags::FOCUSABLE);
    assert!(document.store().core(first).has_flags(NodeFlags::FOCUSABLE));
}

#[test]
fn a_column_is_readable_and_writable_through_the_node_name() {
    let (mut document, [_, first, _, _]) = small();
    let key = document.store().key_of(first);

    let mut set = ListenerSet::new();
    set.add(Listener {
        kind: EventKind::Click,
        options: ListenerOptions::DEFAULT,
        id: ListenerId::new(1),
    });
    *document.store_mut().columns_mut().listeners.get_mut(key) = set;

    assert!(
        document
            .store()
            .columns()
            .listeners
            .get(key)
            .expect("the page was allocated")
            .listens_for(EventKind::Click)
    );
}

#[test]
fn two_handles_to_one_node_are_equal_and_two_nodes_are_not() {
    let (document, [_, first, second, _]) = small();
    assert_eq!(document.node(first), document.node(first));
    assert_ne!(document.node(first), document.node(second));
    assert_eq!(
        format!("{:?}", document.node(first)),
        format!("Node({})", first.get())
    );
}
