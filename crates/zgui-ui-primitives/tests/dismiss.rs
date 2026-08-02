//! Dismissal driven through real presses and real key events.

mod harness;

use std::cell::RefCell;
use std::rc::Rc;

use harness::Harness;
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::reactive::UnsyncCallback;
use zgui::vocab::{EventKind, Key, NamedKey};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

/// What each layer under test was told, in order.
type Reasons = Rc<RefCell<Vec<(&'static str, DismissReason)>>>;

/// A layer that writes down why it was dismissed.
#[component]
fn Recording(
    /// Which layer this is, in the record.
    name: &'static str,
    /// Where the reasons go.
    seen: Reasons,
    /// Which band it is on.
    #[prop(default = OverlayLayer::default())]
    layer: OverlayLayer,
    /// The layer's own element.
    element_ref: NodeRef,
    /// What is inside it.
    children: Children,
) -> impl IntoView {
    let record = Rc::clone(&seen);
    view! {
        DismissableLayer(
            layer = layer,
            element_ref = element_ref,
            on_dismiss = UnsyncCallback::new(move |reason: DismissReason| {
                record.borrow_mut().push((name, reason));
            })
        ) {
            {children.into_view_once()}
        }
    }
}

/// A dialog with a menu inside it, both dismissable, and a control outside both.
#[component]
fn Nested(
    /// Where the reasons go.
    seen: Reasons,
    /// The dialog's own element.
    dialog: NodeRef,
    /// The menu's own element.
    menu: NodeRef,
    /// Something to press that is inside neither.
    outside: NodeRef,
    /// Whether the menu is open.
    menu_open: RwSignal<bool, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    let inner = Rc::clone(&seen);
    view! {
        box {
            control(node_ref = outside) {"elsewhere"}
            Recording(name = "dialog", seen = Rc::clone(&seen), layer = OverlayLayer::Modal, element_ref = dialog) {
                if move || menu_open.get() {
                    Recording(
                        name = "menu",
                        seen = Rc::clone(&inner),
                        layer = OverlayLayer::Modal,
                        element_ref = menu
                    ) {
                        control {"item"}
                    }
                } else {}
            }
        }
    }
}

/// Mounts the nested pair, and hands back everything a test drives it with.
fn nested() -> (
    Harness,
    Reasons,
    NodeRef,
    NodeRef,
    RwSignal<bool, zgui::reactive::LocalStorage>,
) {
    let harness = Harness::open();
    let seen: Reasons = Rc::new(RefCell::new(Vec::new()));
    let dialog = harness.window.scope.with(NodeRef::new);
    let menu = harness.window.scope.with(NodeRef::new);
    let outside = harness.window.scope.with(NodeRef::new);
    let menu_open = harness
        .window
        .scope
        .with(|| zgui::reactive::RwSignal::new_local(true));
    let record = Rc::clone(&seen);
    harness.mount(move || {
        view! {
            Nested(
                seen = record,
                dialog = dialog,
                menu = menu,
                outside = outside,
                menu_open = menu_open
            )
        }
    });

    // The engine's answers a test declares: what is inside what, and where the boxes are.
    let (dialog_node, menu_node, outside_node) = (
        dialog.get_untracked().expect("bound"),
        menu.get_untracked().expect("bound"),
        outside.get_untracked().expect("bound"),
    );
    harness.window.host.set_contains(dialog_node, menu_node);
    harness.window.place(outside_node, 0.0, 0.0, 50.0, 20.0);
    harness.window.place(dialog_node, 100.0, 0.0, 200.0, 200.0);
    harness.window.place(menu_node, 120.0, 20.0, 100.0, 100.0);

    (harness, seen, dialog, menu, menu_open)
}

/// Presses the pointer down at a point, which is what an outside press is.
fn press(harness: &Harness, x: f32, y: f32) {
    harness.window.dispatcher().pointer_at(
        zgui::geom::Point::new(zgui::geom::DevicePx(x), zgui::geom::DevicePx(y)),
        EventKind::PointerDown,
    );
}

/// Sends Escape at a node.
fn escape(harness: &Harness, at: NodeRef) {
    let node = at.get_untracked().expect("bound");
    harness
        .window
        .dispatcher()
        .key(node, Key::Named(NamedKey::Escape));
}

#[test]
fn a_press_past_the_innermost_layer_dismisses_only_that_one() {
    // The case the whole layer stack exists for. Dismissing both would close the dialog the user
    // was working in; dismissing the outer one would leave a menu floating over nothing.
    let (harness, seen, _dialog, _menu, _open) = nested();

    // Inside the dialog, outside the menu.
    press(&harness, 110.0, 180.0);

    assert_eq!(
        seen.borrow().as_slice(),
        [("menu", DismissReason::OutsidePress)]
    );
}

#[test]
fn a_press_inside_the_innermost_layer_dismisses_nothing() {
    let (harness, seen, _dialog, _menu, _open) = nested();
    press(&harness, 150.0, 60.0);
    assert!(seen.borrow().is_empty(), "{:?}", seen.borrow());
}

#[test]
fn closing_the_inner_layer_hands_the_next_press_to_the_outer_one() {
    let (harness, seen, _dialog, _menu, menu_open) = nested();

    press(&harness, 110.0, 180.0);
    assert_eq!(seen.borrow().len(), 1);

    // The caller acts on what it was told: the menu really unmounts, and its entry in the stack
    // goes with it.
    menu_open.set(false);
    harness.window.frame();

    press(&harness, 10.0, 10.0);
    assert_eq!(
        seen.borrow().as_slice(),
        [
            ("menu", DismissReason::OutsidePress),
            ("dialog", DismissReason::OutsidePress)
        ]
    );
}

#[test]
fn escape_reaches_the_innermost_layer_and_stops_there() {
    let (harness, seen, _dialog, menu, _open) = nested();
    escape(&harness, menu);
    assert_eq!(
        seen.borrow().as_slice(),
        [("menu", DismissReason::EscapeKey)],
        "one press must not close two surfaces"
    );
}

#[test]
fn a_layer_that_refuses_escape_is_not_dismissed_by_it() {
    let harness = Harness::open();
    let seen: Reasons = Rc::new(RefCell::new(Vec::new()));
    let layer = harness.window.scope.with(NodeRef::new);
    let record = Rc::clone(&seen);
    harness.mount(move || {
        view! {
            DismissableLayer(
                element_ref = layer,
                dismiss_on_escape = false,
                on_dismiss = UnsyncCallback::new(move |reason: DismissReason| {
                    record.borrow_mut().push(("layer", reason));
                })
            ) {
                control {"item"}
            }
        }
    });
    let node = layer.get_untracked().expect("bound");
    harness.window.place(node, 0.0, 0.0, 100.0, 100.0);

    escape(&harness, layer);
    assert!(seen.borrow().is_empty());

    // And the other half still works, so this is not a layer that has stopped listening.
    press(&harness, 400.0, 400.0);
    assert_eq!(
        seen.borrow().as_slice(),
        [("layer", DismissReason::OutsidePress)]
    );
}

#[test]
fn a_layer_that_has_unmounted_hears_nothing() {
    // The listeners live on the window's root, which outlives the layer. Nothing else would ever
    // take them off, so a leak here is a surface that keeps dismissing itself after it has gone.
    let harness = Harness::open();
    let seen: Reasons = Rc::new(RefCell::new(Vec::new()));
    let showing = harness
        .window
        .scope
        .with(|| zgui::reactive::RwSignal::new_local(true));
    let layer = harness.window.scope.with(NodeRef::new);
    let record = Rc::clone(&seen);
    harness.mount(move || {
        view! {
            if move || showing.get() {
                DismissableLayer(
                    element_ref = layer,
                    on_dismiss = UnsyncCallback::new({
                        let record = Rc::clone(&record);
                        move |reason: DismissReason| record.borrow_mut().push(("layer", reason))
                    })
                ) {
                    control {"item"}
                }
            } else {}
        }
    });

    let root_listeners = harness.window.dom.tree().listener_count();
    assert!(root_listeners > 0);

    showing.set(false);
    harness.window.frame();

    press(&harness, 400.0, 400.0);
    assert!(seen.borrow().is_empty(), "a gone layer answered a press");
    assert!(
        harness.window.dom.tree().listener_count() < root_listeners,
        "the listeners on the window's root went with it"
    );
}
