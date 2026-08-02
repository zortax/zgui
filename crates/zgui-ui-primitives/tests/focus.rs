//! Focus trapping and roving focus, driven through real key events.

mod harness;

use harness::Harness;
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

/// One item of a roving group: a control that registers itself and binds its own tab stop.
#[component]
fn Item(
    /// The item's element, handed back to the test.
    element_ref: NodeRef,
    /// What it says.
    label: &'static str,
) -> impl IntoView {
    let item = use_roving_item(element_ref);
    view! {
        control(
            node_ref = element_ref,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:focus_in = move |_| {
                if let Some(item) = item {
                    item.activate();
                }
            }
        ) {
            {label}
        }
    }
}

/// A toolbar of three items.
#[component]
fn Toolbar(
    /// The three items' elements.
    items: [NodeRef; 3],
    /// The group's own element.
    element_ref: NodeRef,
    /// Which way the arrows move.
    #[prop(default = Orientation::Horizontal)]
    orientation: Orientation,
) -> impl IntoView {
    view! {
        RovingFocus(orientation = orientation, element_ref = element_ref) {
            box {
                Item(element_ref = items[0], label = "one")
                Item(element_ref = items[1], label = "two")
                Item(element_ref = items[2], label = "three")
            }
        }
    }
}

/// Mounts a toolbar and tells the host what order its items are in.
fn toolbar(orientation: Orientation) -> (Harness, [NodeRef; 3], NodeRef) {
    let harness = Harness::open();
    let items = [
        harness.window.scope.with(NodeRef::new),
        harness.window.scope.with(NodeRef::new),
        harness.window.scope.with(NodeRef::new),
    ];
    let group = harness.window.scope.with(NodeRef::new);
    harness.mount(move || {
        view! { Toolbar(items = items, element_ref = group, orientation = orientation) }
    });

    let nodes: Vec<_> = items
        .iter()
        .map(|item| item.get_untracked().expect("bound"))
        .collect();
    harness.window.host.set_tree_order(nodes);
    (harness, items, group)
}

/// Sends a key at the group's element, which is where the arrows are listened for.
fn key(harness: &Harness, group: NodeRef, named: NamedKey) {
    let node = group.get_untracked().expect("bound");
    harness.window.dispatcher().key(node, Key::Named(named));
}

/// What the tree says an item's `tabindex` is.
fn tabindex(harness: &Harness, item: NodeRef) -> Option<String> {
    let node = item.get_untracked().expect("bound");
    harness
        .window
        .dom
        .tree()
        .attribute(node, zgui::view::AttrName::new("tabindex"))
}

#[test]
fn one_item_of_a_group_is_the_tab_stop_and_the_rest_are_not() {
    // The whole point of a roving tabindex: a toolbar of three is one thing to tab past.
    let (harness, items, _group) = toolbar(Orientation::Horizontal);
    assert_eq!(tabindex(&harness, items[0]).as_deref(), Some("0"));
    assert_eq!(tabindex(&harness, items[1]).as_deref(), Some("-1"));
    assert_eq!(tabindex(&harness, items[2]).as_deref(), Some("-1"));
}

#[test]
fn an_arrow_key_moves_the_focus_and_the_tab_stop_together() {
    let (harness, items, group) = toolbar(Orientation::Horizontal);
    harness.window.transcript.clear();

    key(&harness, group, NamedKey::ArrowRight);
    harness.window.frame();

    let second = items[1].get_untracked().expect("bound");
    assert!(
        harness
            .window
            .transcript
            .to_string()
            .contains(&format!("focus #{}", second.backend_bits())),
        "{}",
        harness.window.transcript
    );
    assert_eq!(tabindex(&harness, items[0]).as_deref(), Some("-1"));
    assert_eq!(
        tabindex(&harness, items[1]).as_deref(),
        Some("0"),
        "the tab stop followed the focus"
    );
}

#[test]
fn stepping_past_the_last_item_wraps_to_the_first() {
    let (harness, items, group) = toolbar(Orientation::Horizontal);
    for _ in 0..3 {
        key(&harness, group, NamedKey::ArrowRight);
        harness.window.frame();
    }
    assert_eq!(tabindex(&harness, items[0]).as_deref(), Some("0"));
}

#[test]
fn the_end_keys_go_to_the_ends() {
    let (harness, items, group) = toolbar(Orientation::Vertical);
    key(&harness, group, NamedKey::End);
    harness.window.frame();
    assert_eq!(tabindex(&harness, items[2]).as_deref(), Some("0"));

    key(&harness, group, NamedKey::Home);
    harness.window.frame();
    assert_eq!(tabindex(&harness, items[0]).as_deref(), Some("0"));
}

#[test]
fn a_horizontal_group_leaves_the_vertical_arrows_for_whatever_is_outside_it() {
    // A vertical arrow inside a horizontal menubar belongs to the submenu below it. A group that
    // swallowed it would make the submenu unreachable.
    let (harness, items, group) = toolbar(Orientation::Horizontal);
    let delivered = {
        let node = group.get_untracked().expect("bound");
        harness
            .window
            .dispatcher()
            .key(node, Key::Named(NamedKey::ArrowDown))
    };
    harness.window.frame();

    assert_eq!(
        tabindex(&harness, items[0]).as_deref(),
        Some("0"),
        "nothing moved"
    );
    assert!(
        delivered.default.is_allowed(),
        "the key was left alone rather than swallowed"
    );
}

#[test]
fn a_focus_scope_traps_while_it_is_asked_to_and_releases_when_it_is_not() {
    let harness = Harness::open();
    let trapped = harness.window.scope.with(|| RwSignal::new_local(true));
    harness.mount(move || {
        view! {
            FocusScope(trapped = Signal::from(trapped)) {
                control {"inside"}
            }
        }
    });

    assert_eq!(harness.window.host.live_focus_traps(), 1);

    trapped.set(false);
    harness.window.frame();
    assert_eq!(harness.window.host.live_focus_traps(), 0);

    trapped.set(true);
    harness.window.frame();
    assert_eq!(harness.window.host.live_focus_traps(), 1);
}

#[test]
fn a_focus_scope_installs_exactly_one_trap_however_often_it_re_runs() {
    // A trap installed a second time is a stack two deep for one dialog, and closing the dialog
    // then leaves the window trapped with nothing open.
    let harness = Harness::open();
    let trapped = harness.window.scope.with(|| RwSignal::new_local(true));
    harness.mount(move || {
        view! {
            FocusScope(trapped = Signal::from(trapped)) {
                control {"inside"}
            }
        }
    });

    for _ in 0..3 {
        trapped.set(true);
        harness.window.frame();
    }
    assert_eq!(harness.window.host.live_focus_traps(), 1);
}

#[test]
fn a_focus_scope_that_unmounts_takes_its_trap_with_it() {
    // The failure this guards against is a window that can never be tabbed again.
    let harness = Harness::open();
    let showing = harness.window.scope.with(|| RwSignal::new_local(true));
    harness.mount(move || {
        view! {
            if move || showing.get() {
                FocusScope {
                    control {"inside"}
                }
            } else {}
        }
    });
    assert_eq!(harness.window.host.live_focus_traps(), 1);

    showing.set(false);
    harness.window.frame();
    assert_eq!(harness.window.host.live_focus_traps(), 0);
}
