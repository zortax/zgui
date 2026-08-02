//! The three seams, asserted from outside the crate.
//!
//! Everything here is a property a downstream backend author depends on: that each seam is a
//! trait object, that the handle a view keeps stays cheap to copy, and that a handle minted for
//! one window is recognisable as foreign by another.

use std::rc::Rc;

use zgui_interned::ElementName;
use zgui_reactive::{Mounted, install};
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::{
    DiscardCommands, DocumentId, Dom, DomHandle, EventSink, HostHandle, NodeId, NodeRef, ViewHost,
};

#[test]
fn each_of_the_three_seams_is_object_safe() {
    let dom: Rc<dyn Dom> = Rc::new(StubDom::new(DocumentId::FIRST));
    let host: Rc<dyn ViewHost> = Rc::new(StubHost::default());
    let sink: Rc<dyn EventSink> = Rc::new(DiscardCommands);

    let node = dom.create_element(ElementName::new("box"));
    assert_eq!(host.running_animations(node), 0);
    assert_eq!(Rc::strong_count(&sink), 1);
}

#[test]
fn a_node_ref_is_copy() {
    // The handles a `NodeRef` needs live *inside* its signal's value. Beside it they would take
    // `Copy` away, and every component that stores one in a `move` closure would stop compiling.
    const fn assert_copy<T: Copy>() {}
    const _: () = assert_copy::<NodeRef>();

    install().ok();
    let window = Mounted::new();
    let node_ref = window.with(NodeRef::new);
    let copied = node_ref;
    let also_copied = node_ref;
    assert_eq!(copied.get(), also_copied.get());
    window.unmount();
}

#[test]
fn a_handle_from_one_window_is_recognisable_as_foreign_by_another() {
    let first = DocumentId::FIRST;
    let second = DocumentId::new(1).expect("in range");

    let one = StubDom::new(first);
    let other = StubDom::new(second);
    let node = one.create_element(ElementName::new("box"));

    assert!(node.belongs_to(first));
    assert!(!node.belongs_to(other.document()));
}

#[test]
#[should_panic(expected = "belongs to another document")]
fn applying_a_handle_to_the_wrong_document_is_caught_rather_than_corrupting_it() {
    let one = StubDom::new(DocumentId::FIRST);
    let other = StubDom::new(DocumentId::new(1).expect("in range"));
    let node = one.create_element(ElementName::new("box"));
    other.set_text(node, "this is not my node");
}

#[test]
fn two_windows_build_through_their_own_backends_with_no_global_in_between() {
    install().ok();

    let first_backend = Rc::new(StubDom::new(DocumentId::FIRST));
    let second_backend = Rc::new(StubDom::new(DocumentId::new(1).expect("in range")));
    let first = DomHandle::from_rc(first_backend.clone());
    let second = DomHandle::from_rc(second_backend.clone());

    let a = first.create_element(ElementName::new("box"));
    let b = second.create_element(ElementName::new("box"));

    assert_eq!(first_backend.node_count(), 1);
    assert_eq!(second_backend.node_count(), 1);
    assert_ne!(a, b);
    assert!(!first.ptr_eq(&second));
}

#[test]
fn every_imperative_reach_from_a_node_ref_goes_through_the_host_seam() {
    install().ok();
    let engine = Rc::new(StubHost::default());
    let host = HostHandle::from_rc(engine.clone());
    let dom = DomHandle::new(StubDom::new(DocumentId::FIRST));
    let window = Mounted::new();

    let node = dom.create_element(ElementName::new("scroll"));
    let node_ref = window.with(NodeRef::new);
    node_ref.bind(node, &dom, &host);

    node_ref.focus();
    node_ref.scroll_to(
        zgui_view::ScrollTarget::IntoView,
        zgui_view::ScrollBehavior::Smooth,
    );
    node_ref.select_all();

    assert_eq!(engine.scroll_commands().len(), 1);
    assert_eq!(node_ref.selection(), Some(0..usize::MAX));
    window.unmount();
}

#[test]
fn a_node_identity_survives_the_trip_through_a_bare_integer() {
    let dom = StubDom::new(DocumentId::FIRST);
    let node = dom.create_element(ElementName::new("box"));
    assert_eq!(NodeId::from_u64(node.as_u64()), Some(node));
}
