//! What a component test looks like when it is written with this harness.
//!
//! These are the harness's own acceptance: each one is a claim a real component's test would make,
//! made here about a small one, so that a harness that could not express it fails on its own terms
//! rather than when somebody is trying to write the component.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zgui_elements::{control, row};
use zgui_interned::ClassName;
use zgui_interned::ElementName;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_testkit_view::{Command, Op, Window};
use zgui_view::{Anchor, Dom, IntoView, ListenerOptions, NodeId, View, events};
use zgui_vocab::{EventKind, Phase};

/// Builds a view into a window and mounts it, answering with the node it produced.
fn mount(window: &Window, view: impl IntoView) -> (NodeId, Box<dyn Anchor>) {
    let mut built = window
        .scope
        .with(|| view.into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom_handle, window.root, None);
    let node = built.first_node().expect("the view produced a node");
    (node, Box::new(built))
}

#[test]
fn a_press_reaches_the_handler_a_component_registered() {
    let window = Window::open();
    let presses = Rc::new(Cell::new(0));
    let count = Rc::clone(&presses);

    let (button, _held) = mount(
        &window,
        control().on(events::CLICK, move |_| count.set(count.get() + 1)),
    );
    window.place(button, 0.0, 0.0, 80.0, 24.0);

    let delivered = window.click(10.0, 10.0);
    assert_eq!(delivered.target, Some(button));
    assert_eq!(presses.get(), 1);
}

#[test]
fn a_press_that_lands_on_nothing_reaches_nothing() {
    let window = Window::open();
    let presses = Rc::new(Cell::new(0));
    let count = Rc::clone(&presses);
    let (button, _held) = mount(
        &window,
        control().on(events::CLICK, move |_| count.set(count.get() + 1)),
    );
    window.place(button, 0.0, 0.0, 80.0, 24.0);

    let delivered = window.click(200.0, 200.0);
    assert_eq!(delivered.target, None);
    assert!(!delivered.reached_anything());
    assert_eq!(presses.get(), 0);
}

#[test]
fn an_ancestor_watching_on_the_way_down_hears_about_a_press_on_something_else() {
    // What dismissing an open menu by pressing past it is written with, and the reason the way
    // down exists at all: the pressed element cooperates in no way.
    let window = Window::open();
    let seen = Rc::new(Cell::new(0));
    let count = Rc::clone(&seen);

    let (layer, _held) = mount(
        &window,
        row()
            .on_with(events::POINTER_DOWN, ListenerOptions::CAPTURE, move |_| {
                count.set(count.get() + 1)
            })
            .child(control()),
    );
    let button = window.dom.tree().children(layer)[0];
    window.place(layer, 0.0, 0.0, 200.0, 100.0);
    window.place(button, 0.0, 0.0, 80.0, 24.0);

    let delivered = window.dispatcher().pointer_at(
        zgui_geom::Point::new(zgui_geom::DevicePx(10.0), zgui_geom::DevicePx(10.0)),
        EventKind::PointerDown,
    );

    assert_eq!(delivered.target, Some(button));
    assert_eq!(delivered.ran, vec![(layer, Phase::Capture)]);
    assert_eq!(seen.get(), 1);
}

#[test]
fn a_component_that_stops_propagation_keeps_the_press_from_its_ancestors() {
    let window = Window::open();
    let outer = Rc::new(Cell::new(0));
    let outer_count = Rc::clone(&outer);

    let (group, _held) = mount(
        &window,
        row()
            .on(events::CLICK, move |_| {
                outer_count.set(outer_count.get() + 1)
            })
            .child(control().on(events::CLICK, |cx| cx.stop_propagation())),
    );
    let button = window.dom.tree().children(group)[0];
    window.place(group, 0.0, 0.0, 200.0, 100.0);
    window.place(button, 0.0, 0.0, 80.0, 24.0);

    window.click(10.0, 10.0);
    assert_eq!(outer.get(), 0, "the group never heard about it");

    // And a press that misses the button reaches the group, which is what makes the case above a
    // measurement rather than a listener that was never registered.
    window.click(150.0, 50.0);
    assert_eq!(outer.get(), 1);
}

#[test]
fn what_a_handler_asks_for_is_collected_rather_than_carried_out() {
    let window = Window::open();
    let (button, _held) = mount(
        &window,
        control().on(events::POINTER_DOWN, |cx| {
            cx.capture_pointer();
            cx.request_focus(cx.current);
        }),
    );
    window.place(button, 0.0, 0.0, 80.0, 24.0);

    let delivered = window.dispatcher().pointer_at(
        zgui_geom::Point::new(zgui_geom::DevicePx(1.0), zgui_geom::DevicePx(1.0)),
        EventKind::PointerDown,
    );
    assert_eq!(
        delivered.commands,
        vec![
            Command::CapturePointer(button),
            Command::RequestFocus(button)
        ]
    );
}

#[test]
fn a_signal_a_handler_wrote_reaches_the_tree_on_the_next_frame_and_not_before() {
    let window = Window::open();
    let pressed = window.scope.with(|| RwSignal::new(false));

    let (button, _held) = mount(
        &window,
        control()
            .class_toggle(ClassName::new("pressed"), move || pressed.get())
            .on(events::CLICK, move |_| pressed.set(true)),
    );
    window.place(button, 0.0, 0.0, 80.0, 24.0);
    window.frame();
    window.transcript.clear();

    window.click(10.0, 10.0);
    assert!(
        !window
            .dom
            .tree()
            .classes(button)
            .contains(&ClassName::new("pressed")),
        "the write has not been flushed yet"
    );

    window.frame();
    assert!(
        window
            .dom
            .tree()
            .classes(button)
            .contains(&ClassName::new("pressed")),
        "and now it has"
    );
    assert_eq!(
        window.transcript.ops(),
        vec![
            Op::Handler {
                node: button,
                event: "click".to_owned(),
                phase: "target".to_owned(),
            },
            Op::ToggleClass {
                node: button,
                class: "pressed".to_owned(),
                on: true,
            },
        ],
        "and the transcript holds both, in the order they happened"
    );
}

#[test]
fn a_delay_is_measured_in_the_harnesss_clock_and_costs_the_test_nothing() {
    let window = Window::open();
    let shown = window.scope.with(|| RwSignal::new(false));
    let fired = Rc::new(Cell::new(false));
    let flag = Rc::clone(&fired);

    // Held: dropping the handle cancels the callback, which is the behaviour a tooltip that is
    // unmounted mid-delay depends on.
    let _pending = window.scope.with(|| {
        zgui_view::time::set_timeout(core::time::Duration::from_millis(700), move || {
            flag.set(true);
            shown.set(true);
        })
    });

    window.advance(core::time::Duration::from_millis(699));
    assert!(!fired.get());
    window.advance(core::time::Duration::from_millis(1));
    assert!(fired.get());
    assert!(shown.get_untracked());
    assert_eq!(window.now(), core::time::Duration::from_millis(700));
}

#[test]
fn an_order_names_registrations_by_identity_so_a_swap_mid_dispatch_runs_nothing_new() {
    // A handler is entitled to change what is registered on another element while the event is
    // still travelling — dismissing a layer and installing its replacement is exactly that. An
    // order that named "the group's first click handler" rather than an identity would then run
    // the *replacement* in the removed one's place, in the same dispatch that installed it.
    let window = Window::open();
    let ran: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));

    let group = window.dom.create_element(ElementName::new("row"));
    window.dom.insert(window.root, group, None);
    let button = window.dom.create_element(ElementName::new("control"));
    window.dom.insert(group, button, None);
    window.place(group, 0.0, 0.0, 200.0, 100.0);
    window.place(button, 0.0, 0.0, 80.0, 24.0);

    let doomed = {
        let ran = Rc::clone(&ran);
        window.dom.add_listener(
            group,
            EventKind::Click,
            ListenerOptions::DEFAULT,
            Rc::new(move |_| ran.borrow_mut().push("doomed")),
        )
    };
    {
        let ran = Rc::clone(&ran);
        window.dom.add_listener(
            group,
            EventKind::Click,
            ListenerOptions::DEFAULT,
            Rc::new(move |_| ran.borrow_mut().push("survivor")),
        );
    }
    {
        let ran = Rc::clone(&ran);
        let dom = Rc::clone(&window.dom);
        window.dom.add_listener(
            button,
            EventKind::Click,
            ListenerOptions::DEFAULT,
            Rc::new(move |_| {
                ran.borrow_mut().push("button");
                dom.remove_listener(group, doomed);
                let ran = Rc::clone(&ran);
                dom.add_listener(
                    group,
                    EventKind::Click,
                    ListenerOptions::DEFAULT,
                    Rc::new(move |_| ran.borrow_mut().push("replacement")),
                );
            }),
        );
    }

    window.click(10.0, 10.0);
    assert_eq!(
        ran.borrow().as_slice(),
        &["button", "survivor"],
        "the removed registration does not run, and the one installed during the dispatch is not \
         run in its place"
    );
}
