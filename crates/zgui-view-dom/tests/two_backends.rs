//! The seam has two implementations, and this is the test that says so rather than asserting it.
//!
//! One view, one build, two backends: a real document with a style engine underneath it, and an
//! in-memory tree with nothing underneath it at all. The view code is the same code — it is not
//! parameterised by the backend and cannot see which one it is driving — so what this pins is that
//! the abstraction is load-bearing rather than decorative.

mod support;

use std::rc::Rc;

use zgui_interned::{AttrName, ClassName, Ident};
use zgui_reactive::prelude::*;
use zgui_reactive::{Mounted, RwSignal, flush, install};
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::{
    Anchor, BuildCxOwned, DocumentId, DomHandle, HostHandle, IntoView, NodeId, UiState, View,
    events,
};

use crate::support::Window;

/// The one view both backends are driven by.
///
/// It uses every kind of write the seam has: a class list, a toggled class, an attribute, an
/// author-defined state, an interaction state, a listener, a text child that follows a signal.
fn panel(open: RwSignal<bool>) -> impl IntoView {
    zgui_elements::column()
        .class("panel")
        .class_toggle(ClassName::new("open"), open)
        .attribute(AttrName::new("data-part"), "root")
        .custom_state(Ident::new("peeking"), open)
        .child(
            zgui_elements::control()
                .state(UiState::DISABLED, move || !open.get())
                .on(events::CLICK, move |_| {
                    open.update(|value| *value = !*value)
                })
                .child(move || if open.get() { "close" } else { "open" }.to_owned()),
        )
}

/// What the two backends are compared on: the shape of the tree and the values on it.
#[derive(Debug, PartialEq)]
struct Snapshot {
    classes: Vec<String>,
    part: Option<String>,
    peeking: bool,
    disabled: bool,
    text: String,
}

#[test]
fn the_same_view_drives_an_in_memory_tree_and_a_real_document_identically() {
    install().ok();

    // The in-memory backend.
    let stub_backend = Rc::new(StubDom::new(DocumentId::FIRST));
    let stub_dom = DomHandle::from_rc(stub_backend.clone());
    let stub_window = Mounted::new();
    let stub_cx = BuildCxOwned::new(
        stub_dom.clone(),
        HostHandle::new(StubHost::default()),
        stub_window.owner().clone(),
        DocumentId::FIRST,
    );
    let stub_root = stub_dom.create_element(zgui_interned::ElementName::new("root"));
    let stub_open = stub_window.with(|| RwSignal::new(false));
    let mut stub_built = stub_window.with(|| panel(stub_open).into_view().build(&mut stub_cx.cx()));
    stub_built.mount(&stub_dom, stub_root, None);
    let stub_node = stub_built.first_node().expect("the panel made a node");

    // The real document.
    let real = Window::open();
    let real_open = real.window.with(|| RwSignal::new(false));
    let mut real_built = real
        .window
        .with(|| panel(real_open).into_view().build(&mut real.cx.cx()));
    real_built.mount(&real.dom, real.root, None);
    let real_node = real_built.first_node().expect("the panel made a node");

    let read_stub = |node: NodeId| Snapshot {
        classes: stub_backend
            .classes(node)
            .iter()
            .map(|class| class.as_str().to_owned())
            .collect(),
        part: stub_backend.attribute(node, AttrName::new("data-part")),
        peeking: stub_backend.has_custom_state(node, Ident::new("peeking")),
        disabled: {
            let control = stub_backend.children(node)[0];
            stub_backend.ui_state(control).contains(UiState::DISABLED)
        },
        text: stub_backend.text_content(node),
    };
    let read_real = |node: NodeId| Snapshot {
        classes: real
            .backend
            .classes(node)
            .iter()
            .map(|class| class.as_str().to_owned())
            .collect(),
        part: real.backend.attribute(node, AttrName::new("data-part")),
        peeking: real.backend.has_custom_state(node, Ident::new("peeking")),
        disabled: {
            let control = real.backend.children(node)[0];
            real.backend.ui_state(control).contains(UiState::DISABLED)
        },
        text: real.backend.text_content(node),
    };

    let closed = read_stub(stub_node);
    assert_eq!(closed, read_real(real_node), "closed");
    assert_eq!(closed.classes, vec!["panel".to_owned()]);
    assert!(closed.disabled);
    assert_eq!(closed.text, "open");

    stub_open.set(true);
    real_open.set(true);
    flush();

    let opened = read_stub(stub_node);
    assert_eq!(opened, read_real(real_node), "opened");
    assert_eq!(opened.classes, vec!["panel".to_owned(), "open".to_owned()]);
    assert!(opened.peeking);
    assert!(!opened.disabled);
    assert_eq!(opened.text, "close");
    assert_ne!(closed, opened, "the signal actually changed something");

    stub_built.unmount(&stub_dom);
    real_built.unmount(&real.dom);
    stub_window.unmount();
    real.window.unmount();
}
