//! Every element name and every attribute form, compiled against the real vocabulary.
//!
//! This is the fixture that makes the token-level lowering tests mean something. Those compare the
//! expansion against a string, and a string is free to name a builder method that does not exist,
//! take an argument of the wrong type, or call a function whose name is a reserved word. Here the
//! expansion is compiled and run.

extern crate zgui_elements as zgui;

use zgui_elements::Focus;
use zgui_reactive::{Mounted, RwSignal, install};
use zgui_view::prelude::*;
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::{
    Anchor, Attrs, BuildCxOwned, DocumentId, DomHandle, HostHandle, Role, View,
};
use zgui_view_macro::view;
use std::rc::Rc;

fn main() {
    install().ok();
    let backend = Rc::new(StubDom::new(DocumentId::FIRST));
    let dom = DomHandle::from_rc(backend.clone());
    let window = Mounted::new();
    let cx = BuildCxOwned::new(
        dom.clone(),
        HostHandle::new(StubHost::default()),
        window.owner().clone(),
        DocumentId::FIRST,
    );
    let root = dom.create_element(zgui_view::ElementName::new("root"));

    let open = window.with(|| RwSignal::new(false));
    let handle = window.with(NodeRef::new);
    let forwarded = Attrs::new().class_toggle(zgui_view::ClassName::new("forwarded"), true);

    let tree = view! {
        box(
            class = "page",
            class:open = open,
            style = "gap:1rem",
            style:padding = "4px",
            var:--zgui-fill = "red",
            attr:data-part = "root",
            prop:value = "typed",
            state:disabled = move || !open.get(),
            custom_state:peeking = open,
            node_ref = handle,
            a11y:role = Role::Group,
            a11y:label = "Everything",
            on:click:stop = move |_| open.update(|value| *value = !*value),
            {..forwarded}
        ) {
            row {column {stack()}}
            text {"a run of text"}
            label {"a name"}
            image()
            vector()
            scroll()
            canvas()
            editor()
            field()
            control(tabindex = Focus::Sequential) {"press"}
            // How an element is reached follows a signal like any other attribute: this is what a
            // disabled control and a composite control's roving item both need.
            control(tabindex = move || if open.get() { Focus::Sequential } else { Focus::Programmatic })
            surface()
            spacer()
            overlay-root()
        }
    };

    let mut built = window.with(|| tree.into_view().build(&mut cx.cx()));
    built.mount(&dom, root, None);
    assert!(handle.get_untracked().is_some(), "the handle was bound");
    assert_eq!(backend.text_content(root), "a run of texta namepress");

    built.unmount(&dom);

    // The two control-flow tags. They are written as tags rather than as blocks, which only works
    // if each has a props builder behind it, and the list is keyed by what its `key` prop says.
    let rows = window.with(|| RwSignal::new(vec![1_i32, 2, 3]));
    let flow = view! {
        column {
            Show(when = move || open.get(), fallback = || view! { text {"shut"} }) {
                text {"open"}
            }
            for row in move || rows.get(), key = |row: &i32| *row {
                text {{row.to_string()}}
            }
        }
    };
    let mut flowing = window.with(|| flow.into_view().build(&mut cx.cx()));
    flowing.mount(&dom, root, None);
    assert_eq!(backend.text_content(root), "shut123");

    open.set(true);
    rows.set(vec![3, 1]);
    zgui_reactive::flush();
    assert_eq!(backend.text_content(root), "open31");

    flowing.unmount(&dom);
    window.unmount();
}
