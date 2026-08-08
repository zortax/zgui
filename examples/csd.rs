//! A window that draws its own frame.
//!
//! Run it with `cargo run -p zgui-examples --example csd`.
//!
//! A window with the desktop's decorations turned off owes the user everything the title bar would
//! have given them, and none of it can be done by moving the window from inside the application — a
//! Wayland compositor never lets a window place itself. Each affordance therefore asks the desktop
//! to take over the gesture instead.
//!
//! What it is worth reading for:
//!
//! * `move_drag_handler` on a self-drawn title bar: a press starts a desktop-driven move, and a
//!   second press inside the double-press interval maximises instead — detected press-to-press,
//!   because once the drag begins the compositor owns the pointer and no release ever arrives;
//! * `resize_drag_handler` on eight border strips, one per edge and corner;
//! * buttons inside the title bar work because a control that handles its own press stops the event
//!   before the bar sees it;
//! * the grips hide themselves while the window is maximised, where there is nothing to drag;
//! * on a desktop that refuses any of this — resizing from an edge on macOS — the call does nothing
//!   and the rest of the window still works.

use zgui::prelude::*;

/// The window's own title bar, and the eight edges around its content.
#[component]
fn Frame() -> impl IntoView {
    let window = use_window();
    let maximise_label = {
        let window = window.clone();
        move || {
            if window.maximized().get() {
                "❐"
            } else {
                "▢"
            }
        }
    };
    let extent = {
        let window = window.clone();
        move || {
            let size = window.size().get();
            format!("{:.0} x {:.0}", size.width.0, size.height.0)
        }
    };
    let placement = {
        let window = window.clone();
        move || match window.position() {
            // Every Wayland compositor answers this way: a window there is never told where it
            // was put.
            None => "this desktop does not say where the window is".to_owned(),
            Some(at) => format!("at {:.0}, {:.0}", at.x.0, at.y.0),
        }
    };

    view! {
        column(class = "frame") {
            row(
                class = "titlebar",
                on:pointer_down = window.move_drag_handler()
            ) {
                label(class = "titlebar__name") {"Client-side decorations"}
                row(class = "titlebar__controls") {
                    control(
                        class = "chip",
                        tabindex = Focus::Sequential,
                        a11y:label = "Minimise",
                        // Without this the press reaches the bar behind it and starts a drag, and
                        // the click this button is waiting for is never formed.
                        on:pointer_down = window.no_drag_handler(),
                        on:click = {
                            let window = window.clone();
                            move |_| window.minimize()
                        }
                    ) {"–"}
                    control(
                        class = "chip",
                        tabindex = Focus::Sequential,
                        a11y:label = "Maximise",
                        on:pointer_down = window.no_drag_handler(),
                        on:click = {
                            let window = window.clone();
                            move |_| window.toggle_maximized()
                        }
                    ) {{maximise_label}}
                    control(
                        class = "chip chip--close",
                        tabindex = Focus::Sequential,
                        a11y:label = "Close",
                        on:pointer_down = window.no_drag_handler(),
                        on:click = {
                            let window = window.clone();
                            move |_| window.close()
                        }
                    ) {"✕"}
                }
            }

            column(class = "content") {
                label(class = "eyebrow") {"No desktop title bar"}
                label(class = "hint") {"Drag the bar above to move. Press it twice to maximise."}
                label(class = "hint") {"Drag any edge or corner to resize."}
                label(class = "readout") {{extent}}
                label(class = "readout") {{placement}}
            }

            // One strip per edge and corner. Each asks the desktop to resize from its own side.
            Grip(edge = ResizeEdge::North, class = "grip grip--n")
            Grip(edge = ResizeEdge::South, class = "grip grip--s")
            Grip(edge = ResizeEdge::West, class = "grip grip--w")
            Grip(edge = ResizeEdge::East, class = "grip grip--e")
            Grip(edge = ResizeEdge::NorthWest, class = "grip grip--nw")
            Grip(edge = ResizeEdge::NorthEast, class = "grip grip--ne")
            Grip(edge = ResizeEdge::SouthWest, class = "grip grip--sw")
            Grip(edge = ResizeEdge::SouthEast, class = "grip grip--se")
        }
    }
}

/// One edge or corner the window can be resized from.
#[component]
fn Grip(
    /// Which side of the window this is.
    edge: ResizeEdge,
    /// Where it sits, and how large it is.
    class: &'static str,
) -> impl IntoView {
    let window = use_window();
    let cursor = cursor_for(edge);
    // There is nothing to drag while the window fills the screen. Each grip reads this for itself
    // rather than being handed it: reading a signal is what subscribes to it, and a grip that
    // subscribes is a grip that appears and disappears on its own.
    let framed = {
        let window = window.clone();
        move || !window.maximized().get() && window.fullscreen().get().is_none()
    };

    view! {
        Show(when = framed) {
            box(
                class = class,
                on:pointer_down = window.resize_drag_handler(edge),
                // Imperative rather than from the sheet: a cursor is not a style this engine reads,
                // and setting it on the press would change it after the drag had already begun.
                on:pointer_enter = {
                    let window = window.clone();
                    move |_| window.set_cursor(cursor)
                },
                on:pointer_leave = {
                    let window = window.clone();
                    move |_| window.set_cursor(CursorStyle::Default)
                }
            ) {}
        }
    }
}

/// What the pointer should look like over one edge.
fn cursor_for(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::North | ResizeEdge::South => CursorStyle::ResizeNorthSouth,
        ResizeEdge::East | ResizeEdge::West => CursorStyle::ResizeEastWest,
        ResizeEdge::NorthWest | ResizeEdge::SouthEast => CursorStyle::ResizeNorthWestSouthEast,
        _ => CursorStyle::ResizeNorthEastSouthWest,
    }
}

/// How it looks. The window is transparent so its own rounded corners show the desktop through.
const SHEET: &str = css!(
    ":root {
        background-color: transparent;
        color: #e8ecf4;
        font-family: sans-serif;
        display: block;
    }

    .frame {
        position: relative;
        width: 100%;
        height: 100%;
        border-radius: 12px;
        border: 1px solid #2f3646;
        background-color: #12141a;
        overflow: hidden;
    }

    .titlebar {
        align-items: center;
        justify-content: space-between;
        padding: 0 8px 0 14px;
        height: 38px;
        background-color: #191d26;
        border-bottom: 1px solid #262b36;
    }

    .titlebar__name {
        font-size: 13px;
        color: #b9c2d4;
    }

    .titlebar__controls { gap: 6px; }

    .chip {
        width: 26px;
        height: 26px;
        border-radius: 7px;
        background-color: #232936;
        color: #b9c2d4;
        font-size: 13px;
        line-height: 26px;
        text-align: center;
    }

    .chip:hover { background-color: #2f3646; }

    .chip--close:hover {
        background-color: #d64550;
        color: #ffffff;
    }

    .content {
        align-items: center;
        justify-content: center;
        gap: 8px;
        padding: 28px;
    }

    .eyebrow {
        font-size: 12px;
        letter-spacing: 2px;
        color: #7d879b;
    }

    .hint, .readout {
        font-size: 12px;
        color: #7d879b;
        text-align: center;
    }

    .grip { position: absolute; }

    .grip--n { top: 0; left: 6px; right: 6px; height: 5px; }
    .grip--s { bottom: 0; left: 6px; right: 6px; height: 5px; }
    .grip--w { left: 0; top: 6px; bottom: 6px; width: 5px; }
    .grip--e { right: 0; top: 6px; bottom: 6px; width: 5px; }

    .grip--nw { top: 0; left: 0; width: 10px; height: 10px; }
    .grip--ne { top: 0; right: 0; width: 10px; height: 10px; }
    .grip--sw { bottom: 0; left: 0; width: 10px; height: 10px; }
    .grip--se { bottom: 0; right: 0; width: 10px; height: 10px; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Csd")
        .with_title("Client-side decorations")
        .with_size(560.0, 380.0)
        .with_min_size(320.0, 200.0)
        // The two that make the window the application's to draw.
        .with_decorations(Decorations::None)
        .with_transparent(true)
        .with_stylesheet(SHEET)
        .run(|| view! { Frame() })
}
