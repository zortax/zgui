//! What a caller writes on a component call, read back off the element it lands on.
//!
//! The merge *rules* are checked against a transcript in the macro's own crate, and a transcript is
//! a list of the entries a bundle holds — which is true of an entry whose name no lookup can find.
//! Here the bundle is replayed onto a real element and each entry is read back under the name a
//! sheet would ask for, so a name that survives to the element under the wrong spelling fails.

extern crate zgui_elements as zgui;

use std::rc::Rc;

use zgui_reactive::{Mounted, install};
use zgui_view::prelude::*;
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::{
    Anchor, AttrName, Attrs, BuildCxOwned, CustomPropertyName, DocumentId, DomHandle, ElementName,
    HostHandle, NodeId, View,
};
use zgui_view_macro::{component, view};

/// A component that puts whatever its caller forwarded onto one element of its own.
#[component]
fn Wrapper(
    /// Where to record that element.
    node_ref: NodeRef,
    /// What the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    view! { box(node_ref = node_ref, {..attrs}) }
}

#[test]
fn a_custom_property_written_on_a_component_call_reaches_the_element_the_bundle_lands_on() {
    install().ok();
    let backend = Rc::new(StubDom::new(DocumentId::FIRST));
    let dom = DomHandle::from_rc(backend.clone());
    let host = HostHandle::new(StubHost::default());
    let window = Mounted::new();
    window.with(|| zgui_view::provide_host(host.clone()));
    let cx = BuildCxOwned::new(dom.clone(), host, window.owner().clone(), DocumentId::FIRST);
    let root = dom.create_element(ElementName::new("root"));

    let element = window.with(NodeRef::new);
    let mut state = window.with(|| {
        view! {
            Wrapper(
                node_ref = element,
                var:--brand = "red",
                attr:data-thing = "yes"
            )
        }
        .into_view()
        .build(&mut cx.cx())
    });
    state.mount(&dom, root, None);

    let node: NodeId = element
        .get_untracked()
        .expect("the wrapper built its element");
    // The two are asserted together because the report that found this had them side by side: the
    // attribute landed and the custom property did not, on the one call.
    assert_eq!(
        backend
            .custom_property(node, CustomPropertyName::new("brand"))
            .as_deref(),
        Some("red"),
        "a `var:` on a component call is stored under the name a sheet asks for"
    );
    assert_eq!(
        backend
            .attribute(node, AttrName::new("data-thing"))
            .as_deref(),
        Some("yes")
    );
    // The dashes belong to the declaration and not to the name, so nothing is stored under them.
    assert_eq!(
        backend.custom_property(node, CustomPropertyName::new("--brand")),
        None
    );

    window.unmount();
}
