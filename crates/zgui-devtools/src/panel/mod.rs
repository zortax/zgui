//! The overlay itself: the chord that opens it, the tab strip, and what each tab draws.

mod element;
mod frame;
mod highlight;
pub(crate) mod icon;
mod memory;
mod parity;
mod reactive;
mod sheet;
mod stage;
mod timeline;
mod tree;

use crate::panel::sheet::{SHEET, SHEET_NAME};

use zgui::prelude::*;
use zgui::reactive::{RenderEffect, on_cleanup_local};
use zgui::view::{NodeId, NodeRef, WindowShortcut};
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};

#[allow(
    unused_imports,
    reason = "each tag in the body names the component, and the macro names its props type"
)]
use crate::panel::frame::{FramePanel, FramePanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::memory::{MemoryPanel, MemoryPanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::parity::{ParityPanel, ParityPanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::reactive::{ReactivePanel, ReactivePanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::highlight::{HighlightOverlay, HighlightOverlayProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::timeline::{TimelinePanel, TimelinePanelProps};
#[allow(unused_imports, reason = "as above")]
use crate::panel::tree::{TreePanel, TreePanelProps};
use crate::state::{DevTools, MIN_APP, MIN_WIDTH, Tab};

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
    // The panel's own rules, installed from the panel itself. Idempotent by name, so a second
    // inspector — or a second mount of this one — puts back exactly what is already there.
    install_stylesheet(SHEET_NAME, SHEET);

    let open = tools.open;
    let picking = tools.picking;
    let frozen = tools.frozen;
    let tab = tools.tab;
    let width = tools.width;
    // Where the pointer went down and how wide the panel was then, so the drag is measured from
    // where it started rather than accumulated frame by frame — an accumulated drag drifts by
    // whatever the clamp took off, and ends up somewhere the pointer is not.
    let dragging = RwSignal::new_local(None::<(f32, f64)>);
    // The divider itself, so the clamp can measure the width it takes rather than assume it.
    let grip = NodeRef::new();
    // The panel's own column, which is how picking tells the inspector from the application it is
    // docked beside — both are under the host these listeners are on.
    let panel = NodeRef::new();
    // The application's own wrapper. The tree starts inside it, so what a reader sees at the top of
    // the tree is what they wrote rather than five levels of the framework's and the inspector's
    // own scaffolding.
    let app = NodeRef::new();

    // What the panel's own column is, published for the sampler: the tree tab has to leave the
    // inspector out of the document it draws, and a `NodeRef` is not something a probe can read.
    let publish_panel = RenderEffect::new(move |_: Option<()>| {
        let node = panel.get();
        if tools.panel.get_untracked() != node {
            tools.panel.set(node);
        }
        let inside = app.get();
        if tools.app.get_untracked() != inside {
            tools.app.set(inside);
        }
    });
    on_cleanup_local(move || drop(publish_panel));

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
                        if !tools.picking.get_untracked() {
                            clear_outline(tools);
                        }
                        true
                    }
                    (Key::Named(NamedKey::Escape), _, _) if tools.picking.get_untracked() => {
                        tools.picking.set(false);
                        clear_outline(tools);
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
                if tools.picking.get_untracked() && !ours(panel, ev.target) {
                    aim(tools, ev.target);
                }
            },
            on:pointer_down:capture = move |ev| {
                if tools.picking.get_untracked() && !ours(panel, ev.target) {
                    aim(tools, ev.target);
                    tools.picking.set(false);
                    // Picking is over, so nothing is being aimed at any more.
                    if tools.highlighted.get_untracked().is_some() {
                        tools.highlighted.set(None);
                    }
                    ev.stop_propagation();
                    ev.prevent_default();
                }
            }
        ) {
            column(
                class = "zgui-devtools-app",
                node_ref = app,
                class:zgui-devtools-app-docked = move || open.get()
            ) {
                {children.into_view_once()}
            }
            if move || open.get() {
                // The rule between the two, and the thing that is dragged. A `control` rather than
                // a box because it is focusable: a panel that can only be resized with a pointer is
                // one a keyboard cannot put back after it has been dragged too far.
                control(
                    class = "zgui-devtools__divider",
                    node_ref = grip,
                    tabindex = {Focus::Sequential},
                    a11y:label = "Resize the panel",
                    on:key_down = move |ev| {
                        let step = match &ev.key {
                            Key::Named(NamedKey::ArrowLeft) => STEP,
                            Key::Named(NamedKey::ArrowRight) => -STEP,
                            _ => return,
                        };
                        resize(tools, anchor, grip, width.get_untracked() + step);
                        ev.prevent_default();
                        ev.stop_propagation();
                    },
                    on:pointer_down = move |ev| {
                        ev.capture_pointer();
                        dragging.set(Some((ev.position.x.0, width.get_untracked())));
                        ev.stop_propagation();
                        ev.prevent_default();
                    },
                    on:pointer_move = move |ev| {
                        let Some((start, was)) = dragging.get_untracked() else {
                            return;
                        };
                        // Leftwards is wider: the panel is on the right, so the divider moving
                        // towards the application is the panel taking more of the window.
                        resize(tools, anchor, grip, was + f64::from(start - ev.position.x.0));
                        ev.stop_propagation();
                    },
                    on:pointer_up = move |ev| {
                        dragging.set(None);
                        ev.release_pointer();
                    },
                    on:pointer_cancel = move |ev| {
                        dragging.set(None);
                        ev.release_pointer();
                    }
                ) {
                    box(class = "zgui-devtools__divider-line")
                }
                HighlightOverlay(tools = tools)
                column(
                    class = "zgui-devtools",
                    node_ref = panel,
                    a11y:label = "Inspector",
                    style:width = move || Some(format!("{:.0}px", width.get()))
                ) {
                    row(class = "zgui-devtools__bar") {
                        for which in || Tab::ALL, key = |which: &Tab| *which {
                            control(
                                class = "zgui-devtools__tab",
                                class:zgui-devtools__tab-on = move || tab.get() == which,
                                a11y:label = which.label(),
                                on:click = move |_| tab.set(which)
                            ) {
                                vector(
                                    class = "zgui-devtools__tab-icon",
                                    prop:d = which.icon(),
                                    prop:viewBox = icon::VIEW_BOX
                                )
                                text(class = "zgui-devtools__tab-label") {{which.label()}}
                            }
                        }
                        row(class = "zgui-devtools__spacer")
                        control(
                            class = "zgui-devtools__toggle",
                            class:zgui-devtools__toggle-on = move || picking.get(),
                            a11y:label = "Pick an element",
                            on:click = move |_| {
                                picking.update(|on| *on = !*on);
                                if !picking.get_untracked() {
                                    clear_outline(tools);
                                }
                            }
                        ) {
                            vector(
                                class = "zgui-devtools__icon",
                                prop:d = icon::PICK,
                                prop:viewBox = icon::VIEW_BOX
                            )
                        }
                        control(
                            class = "zgui-devtools__toggle",
                            class:zgui-devtools__toggle-on = move || frozen.get(),
                            a11y:label = "Freeze the panel",
                            on:click = move |_| frozen.update(|on| *on = !*on)
                        ) {
                            vector(
                                class = "zgui-devtools__icon",
                                prop:d = icon::FREEZE,
                                prop:viewBox = icon::VIEW_BOX
                            )
                        }
                    }
                    column(class = "zgui-devtools__tabs") {
                        if move || tab.get() == Tab::Elements {
                            TreePanel(tools = tools)
                        }
                        if move || tab.get() == Tab::Frame {
                            FramePanel(tools = tools)
                        }
                        if move || tab.get() == Tab::Timeline {
                            TimelinePanel(tools = tools)
                        }
                        if move || tab.get() == Tab::Reactivity {
                            ReactivePanel(tools = tools)
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

/// How far one arrow key moves the divider, in CSS pixels.
const STEP: f64 = 16.0;

/// Sets the panel's width to `wanted`, as far as the window allows.
///
/// Clamped at both ends against the window the host fills: never narrower than [`MIN_WIDTH`],
/// because below that the panel stops being readable, and never so wide that the application is
/// left less than [`MIN_APP`] — a drag that covered the window would hide the thing being inspected
/// and take the divider off the edge with it, so there would be no way back.
///
/// Written only when it changes, like everything else the inspector writes: a drag that has reached
/// the clamp goes on delivering pointer moves, and each one of those setting the signal to the
/// value it already holds would keep the window awake for as long as somebody held the button down.
fn resize(tools: DevTools, anchor: NodeRef, grip: NodeRef, wanted: f64) {
    // The host's own box, in CSS pixels: the pointer reports in those, and the width this writes is
    // declared in them, so the ceiling has to be measured in them too. The divider is taken off as
    // well, because it is a flex sibling of both — what is left for the application is the window
    // less the panel *and* the rule between them, and a ceiling that forgot the rule would leave
    // the application a few pixels under the floor this exists to hold it above.
    let scale = anchor.scale();
    let rule = grip
        .bounds()
        .map_or(0.0, |grip| f64::from(grip.size.width.0 / scale));
    let ceiling = anchor
        .bounds()
        .map(|host| f64::from(host.size.width.0 / scale) - MIN_APP - rule)
        .unwrap_or(f64::MAX)
        .max(MIN_WIDTH);
    let next = wanted.clamp(MIN_WIDTH, ceiling);
    if tools.width.get_untracked() != next {
        tools.width.set(next);
    }
}

/// Stops outlining anything, which is what leaving picking means.
fn clear_outline(tools: DevTools) {
    if tools.highlighted.get_untracked().is_some() {
        tools.highlighted.set(None);
    }
}

/// Whether `node` is part of the inspector's own panel.
///
/// Without this, picking would land on the panel the pointer is being moved across on its way to
/// anything else, and the only element anybody could ever inspect would be the inspector. The
/// listeners are on the host, which wraps the application *and* the panel, so this is the only
/// thing telling the two apart.
fn ours(panel: NodeRef, node: NodeId) -> bool {
    panel.contains(node)
}

/// Points the inspector at `node`, and outlines it while picking is aimed there.
fn aim(tools: DevTools, node: NodeId) {
    if tools.picked.get_untracked() != Some(node) {
        tools.picked.set(Some(node));
    }
    if tools.highlighted.get_untracked() != Some(node) {
        tools.highlighted.set(Some(node));
    }
}
