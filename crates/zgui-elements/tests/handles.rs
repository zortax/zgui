//! A handle written on an element lets go of it when the view that built the element goes away.

extern crate zgui_elements as zgui;

use std::rc::Rc;

use zgui_reactive::{Mounted, install};
use zgui_view::prelude::*;
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::{Anchor, BuildCxOwned, DocumentId, DomHandle, ElementName, HostHandle, View};
use zgui_view_macro::view;

/// A window over a stub tree, a context to build in and a root to mount under.
fn window() -> (DomHandle, Mounted, BuildCxOwned, zgui_view::NodeId) {
    install().ok();
    let dom = DomHandle::from_rc(Rc::new(StubDom::new(DocumentId::FIRST)));
    let host = HostHandle::new(StubHost::default());
    let window = Mounted::new();
    window.with(|| zgui_view::provide_host(host.clone()));
    let cx = BuildCxOwned::new(dom.clone(), host, window.owner().clone(), DocumentId::FIRST);
    let root = dom.create_element(ElementName::new("root"));
    (dom, window, cx, root)
}

#[test]
fn dropping_a_built_element_unbinds_its_handle() {
    let (dom, window, cx, root) = window();
    let handle = window.with(NodeRef::new);

    let mut first = window.with(|| {
        view! { box(node_ref = handle) }
            .into_view()
            .build(&mut cx.cx())
    });
    first.mount(&dom, root, None);
    assert!(
        handle.get_untracked().is_some(),
        "the element bound its handle"
    );

    first.unmount(&dom);
    drop(first);
    assert_eq!(
        handle.get_untracked(),
        None,
        "the handle outlives the element unbound"
    );
    window.unmount();
}

#[test]
fn an_old_build_dropped_after_a_new_one_leaves_the_new_binding() {
    let (dom, window, cx, root) = window();
    let handle = window.with(NodeRef::new);

    let mut old = window.with(|| {
        view! { box(node_ref = handle) }
            .into_view()
            .build(&mut cx.cx())
    });
    old.mount(&dom, root, None);
    let mut new = window.with(|| {
        view! { box(node_ref = handle) }
            .into_view()
            .build(&mut cx.cx())
    });
    new.mount(&dom, root, None);
    let bound = handle.get_untracked();

    old.unmount(&dom);
    drop(old);
    assert_eq!(
        handle.get_untracked(),
        bound,
        "the newer element keeps its handle"
    );
    window.unmount();
}
