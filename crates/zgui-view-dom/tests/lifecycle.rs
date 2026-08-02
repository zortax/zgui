//! What a subtree costs the document once it has come and gone.
//!
//! The number this pins is the one nothing else can see. A leaked node is a record and a row in
//! every column it wrote to; a leaked handler is a whole view's captured state, held by an entry
//! naming a node that no longer exists. Neither shows up as a wrong pixel, and both grow without
//! bound in an interface that opens and closes anything.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_elements::{column, control, row, text};
use zgui_reactive::prelude::*;
use zgui_reactive::{RwSignal, flush};
use zgui_view::{Anchor, IntoView, View, events};

use crate::support::Window;

/// A subtree of `rows` rows, each with a listener, a class and a run of text.
fn subtree(rows: usize) -> impl IntoView {
    column().class("list").children(
        (0..rows)
            .map(|index| {
                zgui_view::AnyView::new(
                    row()
                        .class("row")
                        .child(text().child(format!("row {index}")))
                        .child(control().on(events::CLICK, |_| {}).child("x")),
                )
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn a_thousand_node_subtree_leaves_nothing_behind_after_it_has_come_and_gone() {
    let window = Window::open();
    // 250 rows: a list, then per row a row element, a text element, its text node, a control and
    // its text node — a little over a thousand nodes.
    let rows = 250;
    let before_nodes = window.live_nodes();
    let before_handlers = window.backend.handler_count();

    let mut built = window
        .window
        .with(|| subtree(rows).into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    let mounted = window.live_nodes();
    assert!(
        mounted - before_nodes > 1_000,
        "the fixture is meant to be a thousand nodes, and it built {}",
        mounted - before_nodes
    );
    assert_eq!(window.backend.handler_count(), before_handlers + rows);

    built.unmount(&window.dom);
    // Remounting before the frame ends is the ordinary case — a list that moves a row does exactly
    // this — and it must not have cost the nodes anything.
    built.mount(&window.dom, window.root, None);
    assert_eq!(
        window.live_nodes(),
        mounted,
        "a subtree put back before the frame ended was never really removed"
    );

    built.unmount(&window.dom);
    drop(built);
    window.backend.end_frame();

    assert_eq!(
        window.live_nodes(),
        before_nodes,
        "the document is holding nodes nothing can reach"
    );
    assert_eq!(
        window.backend.handler_count(),
        before_handlers,
        "the backend is holding handlers for nodes that no longer exist"
    );
    assert_eq!(window.backend.observation_count(), 0);
    window.window.unmount();
}

/// The counterpart: ending a frame while the subtree is still mounted drops none of it. Without
/// this, the assertion above passes just as well for an `end_frame` that drops everything.
#[test]
fn ending_a_frame_leaves_a_mounted_subtree_exactly_where_it_is() {
    let window = Window::open();
    let before = window.live_nodes();

    let mut built = window
        .window
        .with(|| subtree(20).into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    let mounted = window.live_nodes();
    let handlers = window.backend.handler_count();

    window.backend.end_frame();
    assert_eq!(window.live_nodes(), mounted);
    assert_eq!(window.backend.handler_count(), handlers);
    assert!(mounted > before);

    built.unmount(&window.dom);
    window.window.unmount();
}

/// An element inside a closure is described again every time the closure's signal changes, and a
/// description is a statement of what the element listens for rather than a request to listen once
/// more. Without that, one press of a button that has been re-described a hundred times runs its
/// handler a hundred times — and the table holding those hundred handlers grows for the life of
/// the window.
#[test]
fn describing_an_element_again_replaces_its_listeners_rather_than_adding_a_second_copy() {
    let window = Window::open();
    let label = window.window.with(|| RwSignal::new(0));
    let presses = Rc::new(Cell::new(0));
    let counter = Rc::clone(&presses);

    let view = move || {
        let counter = Rc::clone(&counter);
        control()
            .on(events::CLICK, move |_| counter.set(counter.get() + 1))
            .child(label.get().to_string())
    };
    let before = window.backend.handler_count();
    let mut built = window
        .window
        .with(|| view.into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    let node = built.first_node().expect("the control made a node");
    assert_eq!(window.backend.handler_count(), before + 1);

    for value in 1..=5 {
        label.set(value);
        flush();
        assert_eq!(
            window.backend.handler_count(),
            before + 1,
            "re-describing the control left its previous listener attached"
        );
    }
    assert_eq!(
        window.backend.text_content(node),
        "5",
        "the description that replaced the listener is the one that ran"
    );

    // The registration the document would route a press to, called the way a dispatch calls it.
    let registered: Vec<zgui_dom::side::listeners::ListenerId> = {
        let index = window.backend.index_of(node);
        let document = window.document.borrow();
        let key = document.store().key_of(index);
        document
            .store()
            .columns()
            .listeners
            .get(key)
            .expect("the control listens for something")
            .iter()
            .map(|listener| listener.id)
            .collect()
    };
    assert_eq!(registered.len(), 1, "the document holds one registration");
    for id in registered {
        let handler = window
            .backend
            .handler(id)
            .expect("the registration resolves");
        support::dispatch_click(&handler, node);
    }
    assert_eq!(presses.get(), 1, "one press ran the handler once");

    built.unmount(&window.dom);
    window.window.unmount();
}
