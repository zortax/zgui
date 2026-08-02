//! Drawings: outlines rasterised on the graphics device and composited into the frame.
//!
//! Run it with `cargo run -p zgui-examples --example vector --release`.
//!
//! What it is worth reading for:
//!
//! * `<vector>` is an ordinary element — it has a box, it takes part in layout, it is styled by
//!   ordinary CSS, and it sorts, clips and moves exactly like a background does;
//! * an outline carries no colour of its own, so it takes the element's computed `color` and a
//!   hover rule re-colours it with no new mechanism;
//! * `view-box` is what makes one outline drawable at any size: the curves are written once in a
//!   square of their own and fitted uniformly into whatever box CSS gives the element;
//! * a shape with a counter — a ring — is one outline whose inner contour is wound the other way,
//!   so the non-zero fill rule leaves the middle of it empty and the panel shows through.

use zgui::prelude::*;

/// A ring: an outer circle wound one way, an inner one wound the other.
const RING: &str = "M12 3 C16.971 3 21 7.029 21 12 C21 16.971 16.971 21 12 21 \
                    C7.029 21 3 16.971 3 12 C3 7.029 7.029 3 12 3 Z \
                    M12 5 C8.134 5 5 8.134 5 12 C5 15.866 8.134 19 12 19 \
                    C15.866 19 19 15.866 19 12 C19 8.134 15.866 5 12 5 Z";

/// A tick, which is one closed outline with no counter in it.
const TICK: &str = "M20.5 7.1 L18.9 5.5 L9.6 14.8 L5.1 10.3 L3.5 11.9 L9.6 18.0 Z";

/// A chevron pointing down.
const CHEVRON: &str = "M6.4 9.2 L7.8 7.8 L12 12 L16.2 7.8 L17.6 9.2 L12 14.8 Z";

/// One drawing with a caption under it.
#[component]
fn Mark(
    /// The outlines, in path notation.
    paths: &'static str,
    /// Which size class the drawing takes.
    size: &'static str,
    /// What the caption says.
    caption: &'static str,
) -> impl IntoView {
    view! {
        column(class = "mark") {
            vector(class = "mark__art", class = size, prop:d = paths, prop:viewBox = "0 0 24 24")
            label(class = "mark__caption") {{caption}}
        }
    }
}

/// A panel of drawings at three sizes.
#[component]
fn Gallery() -> impl IntoView {
    view! {
        column(class = "gallery") {
            label(class = "gallery__title") {"Drawings"}
            row(class = "gallery__row") {
                Mark(paths = RING, size = "mark--sm", caption = "ring 24")
                Mark(paths = RING, size = "mark--md", caption = "ring 48")
                Mark(paths = RING, size = "mark--lg", caption = "ring 96")
            }
            row(class = "gallery__row") {
                Mark(paths = TICK, size = "mark--md", caption = "tick")
                Mark(paths = CHEVRON, size = "mark--md", caption = "chevron")
                Mark(paths = RING, size = "mark--md", caption = "hover me")
            }
        }
    }
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .gallery {
        align-items: center;
        gap: 20px;
        padding: 28px 36px;
        border-radius: 16px;
        border: 1px solid #262b36;
        background-color: #191d26;
    }

    .gallery__title {
        font-size: 13px;
        letter-spacing: 2px;
        color: #7d879b;
    }

    .gallery__row { gap: 28px; align-items: flex-end; }

    .mark { align-items: center; gap: 8px; }

    /* No fill is declared, so the outline takes the element's own `color`. */
    .mark__art { color: #6ea8ff; }

    .mark:hover .mark__art { color: #ffb648; }

    .mark--sm { width: 24px; height: 24px; }
    .mark--md { width: 48px; height: 48px; }
    .mark--lg { width: 96px; height: 96px; }

    .mark__caption {
        font-size: 12px;
        color: #7d879b;
    }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Vector")
        .with_title("Drawings")
        .with_size(560.0, 400.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Gallery() })
}
