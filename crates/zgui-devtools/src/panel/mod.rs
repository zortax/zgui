//! The overlay itself: the chord that opens it, the tab strip, and what each tab draws.

mod element;
mod frame;
mod memory;
mod parity;
mod sheet;
mod timeline;

pub use crate::panel::sheet::SHEET;

use zgui::prelude::*;
use zgui::reactive::{RenderEffect, on_cleanup_local};
use zgui::view::{NodeId, NodeRef, WindowShortcut};
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};

#[allow(
    unused_imports,
    reason = "each tag in the body names the component, and the macro names its props type"
)]
use crate::panel::element::{ElementPanel, ElementPanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::frame::{FramePanel, FramePanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::memory::{MemoryPanel, MemoryPanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::parity::{ParityPanel, ParityPanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::timeline::{TimelinePanel, TimelinePanelProps};
use crate::state::{DevTools, Tab};

/// The inspector, docked beside an application's root.
///
/// It draws nothing at all until it is opened, and it is opened with <kbd>F12</kbd>. Open, it is a
/// strip down one side of the window and the application takes the rest — the arrangement every
/// tool of this kind uses, and for the reason that decides it: a panel drawn *over* the page hides
/// the part of the page nearest it, which is usually the part being inspected.
///
/// Docking is what makes the height right as well. The strip is a flex sibling of the application
/// in a container as tall as the viewport, so it is the window's height by construction and its
/// body has something to scroll inside — where a panel positioned against the page would be as
/// tall as the page's content, which is a stub on a short document and runs off the screen on a
/// long one.
///
/// The chord is a **window shortcut**, registered rather than merely listened for. A key is
/// delivered along the path to whatever holds focus, and a window in which nothing holds focus —
/// the state every window launches in — routes one to the document root and no further, so a
/// listener that was only written on this element would be dead until the first click or tab.
/// The registration is what makes <kbd>F12</kbd> work on a window nobody has touched yet.
///
/// Because the listener is a capture listener as well, it sees the chord before the application
/// does and stops it, so an application that binds <kbd>F12</kbd> itself keeps working — and every
/// key that is not one of the inspector's four goes through untouched.
///
/// [`DevTools::set_open`](crate::DevTools::set_open) is there for an application that wants to
/// offer its own way in as well.
#[component]
pub fn Inspector(
    /// The inspector this draws, which is the one the probe writes into.
    tools: DevTools,
    /// The application, which the inspector is docked beside and listens in front of.
    children: Children,
) -> impl IntoView {
    let open = tools.open;
    let picking = tools.picking;
    let frozen = tools.frozen;
    let tab = tools.tab;

    // What hears a key on a window in which nothing has focus. The registration is renewed
    // whenever the element it names is bound — a view that unmounted and mounted again is a
    // different node — and the guard is dropped with the scope, so nothing outlives the document.
    let anchor = NodeRef::new();
    let registration = RenderEffect::new(move |previous: Option<Option<WindowShortcut>>| {
        drop(previous);
        anchor.get();
        anchor.window_shortcut()
    });
    on_cleanup_local(move || drop(registration));

    view! {
        column(
            class = "zgui-devtools-host",
            class:zgui-devtools-host-docked = move || open.get(),
            node_ref = anchor,
            on:key_down:capture = move |ev| {
                let handled = match (&ev.key, ev.modifiers.control(), ev.modifiers.shift()) {
                    (Key::Named(NamedKey::F12), _, _) => {
                        tools.open.update(|showing| *showing = !*showing);
                        true
                    }
                    (Key::Named(NamedKey::F8), _, _) => {
                        tools.frozen.update(|held| *held = !*held);
                        true
                    }
                    (Key::Character(letter), true, true) if letter.eq_ignore_ascii_case("c") => {
                        tools.open.set(true);
                        tools.picking.update(|on| *on = !*on);
                        true
                    }
                    (Key::Named(NamedKey::Escape), _, _) if tools.picking.get_untracked() => {
                        tools.picking.set(false);
                        true
                    }
                    _ => false,
                };
                if handled {
                    ev.stop_propagation();
                    ev.prevent_default();
                }
            },
            on:pointer_move:capture = move |ev| {
                if tools.picking.get_untracked() {
                    aim(tools, ev.target);
                }
            },
            on:pointer_down:capture = move |ev| {
                if tools.picking.get_untracked() {
                    aim(tools, ev.target);
                    tools.picking.set(false);
                    ev.stop_propagation();
                    ev.prevent_default();
                }
            }
        ) {
            column(
                class = "zgui-devtools-app",
                class:zgui-devtools-app-docked = move || open.get()
            ) {
                {children.into_view_once()}
            }
            if move || open.get() {
                column(class = "zgui-devtools", a11y:label = "Inspector") {
                    row(class = "zgui-devtools__bar") {
                        for which in || Tab::ALL, key = |which: &Tab| *which {
                            control(
                                class = "zgui-devtools__tab",
                                class:zgui-devtools__tab-on = move || tab.get() == which,
                                a11y:label = which.label(),
                                on:click = move |_| tab.set(which)
                            ) {
                                {which.label()}
                            }
                        }
                        row(class = "zgui-devtools__spacer")
                        control(
                            class = "zgui-devtools__toggle",
                            class:zgui-devtools__toggle-on = move || picking.get(),
                            a11y:label = "Pick an element",
                            on:click = move |_| picking.update(|on| *on = !*on)
                        ) {
                            "pick"
                        }
                        control(
                            class = "zgui-devtools__toggle",
                            class:zgui-devtools__toggle-on = move || frozen.get(),
                            a11y:label = "Freeze the panel",
                            on:click = move |_| frozen.update(|on| *on = !*on)
                        ) {
                            "freeze"
                        }
                    }
                    column(class = "zgui-devtools__body") {
                        if move || tab.get() == Tab::Element {
                            ElementPanel(tools = tools)
                        }
                        if move || tab.get() == Tab::Frame {
                            FramePanel(tools = tools)
                        }
                        if move || tab.get() == Tab::Timeline {
                            TimelinePanel(tools = tools)
                        }
                        if move || tab.get() == Tab::Parity {
                            ParityPanel()
                        }
                        if move || tab.get() == Tab::Memory {
                            MemoryPanel(tools = tools)
                        }
                    }
                }
            }
        }
    }
}

/// Points the inspector at `node`, unless it is part of the inspector.
///
/// Without the second half, picking would immediately land on the panel the pointer is being moved
/// across on its way to anything else, and the only element anybody could ever inspect would be the
/// inspector.
fn aim(tools: DevTools, node: NodeId) {
    if tools.picked.get_untracked() != Some(node) {
        tools.picked.set(Some(node));
    }
}
