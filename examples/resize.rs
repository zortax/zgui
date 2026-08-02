//! What the window looks like while it is being dragged by a corner.
//!
//! Run it with `cargo run -p zgui-examples --example resize --release`.
//!
//! Every example other than this one is written so that its content sits *inside* the window. This
//! one is written so that its content is pinned to the window's four edges and four corners, which
//! makes one question answerable by looking: does the picture on the screen describe the size the
//! window is now, or the size it was a moment ago?
//!
//! A drag is the hardest case a windowing layer has. The compositor restates the window's size far
//! more often than the display can show a new picture — several hundred times a second on a
//! seventy-five hertz panel — and a program that answers every restatement with a full layout, a
//! full repaint and a swapchain rebuild falls behind, so the corners of what it drew trail the
//! corners of the window it was drawn for. Nothing in this file arranges for that or prevents it;
//! it only makes it visible, because the marks are at the edges rather than in the middle.
//!
//! The button in the centre is here for the opposite reason: an ordinary click, in a window that
//! is not being resized, must be as quick as it was before anything was done about the drag.

use zgui::prelude::*;

/// The window: a border, a marker in each corner, and one button.
#[component]
fn Edges() -> impl IntoView {
    let (lit, set_lit) = signal(false);

    view! {
        column(class = "surface", a11y:role = Role::Group, a11y:label = "Resize probe") {
            row(class = "band") {
                box(class = "corner corner--tl")
                spacer()
                box(class = "corner corner--tr")
            }

            row(class = "middle") {
                box(class = "edge edge--left")
                spacer()
                control(
                    class = "button",
                    class:button-lit = move || lit.get(),
                    tabindex = Focus::Sequential,
                    a11y:label = "Toggle",
                    on:click = move |_| set_lit.update(|on| *on = !*on)
                ) {
                    "click"
                }
                spacer()
                box(class = "edge edge--right")
            }

            row(class = "band") {
                box(class = "corner corner--bl")
                spacer()
                box(class = "corner corner--br")
            }
        }
    }
}

/// How it looks.
///
/// The colours are chosen to be unlike anything else on a desktop, so that a screenshot can be
/// asked whether a pixel came from this window at all. The border is the outermost eight pixels of
/// the surface on every side: a region the renderer failed to draw shows up there as the colour of
/// nothing, and a region drawn for a smaller surface shows up as the border sitting somewhere other
/// than the edge.
const SHEET: &str = css!(
    ":root {
        background-color: #0f0026;
        color: #f6f2ff;
        font-family: sans-serif;
    }

    .surface {
        width: 100%;
        height: 100%;
        border: 8px solid #ff2d95;
        background-image: linear-gradient(135deg, #1b0b3a, #0b2f4a 45%, #08402f);
        justify-content: space-between;
    }

    .band {
        justify-content: space-between;
        align-items: flex-start;
    }

    .middle {
        flex-grow: 1;
        align-items: center;
        justify-content: space-between;
    }

    .corner {
        width: 56px;
        height: 56px;
        background-color: #ffd166;
    }

    .corner--tl { background-color: #ffd166; }
    .corner--tr { background-color: #06d6a0; }
    .corner--bl { background-color: #4cc9f0; }
    .corner--br { background-color: #f72585; }

    .edge {
        width: 24px;
        height: 40%;
        background-color: #b388ff;
    }

    .button {
        padding: 14px 34px;
        border-radius: 10px;
        border: 2px solid #ff2d95;
        background-color: #21103f;
        color: #f6f2ff;
        font-size: 22px;
        line-height: 1;
        text-align: center;
    }

    .button:hover { background-color: #2e1758; }

    .button-lit {
        background-color: #ffd166;
        color: #21103f;
    }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Resize")
        .with_title("Resize")
        .with_size(900.0, 620.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Edges() })
}
