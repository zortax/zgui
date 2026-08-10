//! Pictures: decode-for-display, filtered sampling, and `object-fit`.
//!
//! Run it with `ZGUI_IMAGES_DIR=<dir> cargo run -p zgui-examples --example images`, where the
//! directory holds a `photo.png` and a `portrait.png`. Without the variable it looks beside the
//! current directory.
//!
//! What it is worth reading for:
//!
//! * an `image` element takes a `src` and nothing else — decode, caching and upload are the
//!   framework's business, sized by the box the picture is shown in;
//! * one source shown at several sizes costs one decode per size class, not one per element;
//! * `object-fit` and `object-position` are ordinary declarations, and a rounded corner cuts the
//!   fitted picture exactly as it cuts a background.

use zgui::prelude::*;

/// Where the fixture pictures live.
fn dir() -> String {
    std::env::var("ZGUI_IMAGES_DIR").unwrap_or_else(|_| ".".to_owned())
}

/// One captioned specimen.
#[component]
fn Panel(
    /// What the panel demonstrates.
    #[prop(into)]
    title: String,
    /// What it shows.
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "panel") {
            label(class = "panel__title") {{title}}
            box(class = "panel__body") {{children.into_view_once()}}
        }
    }
}

/// The gallery: one photo at several sizes, and one portrait under each fit.
#[component]
fn Gallery() -> impl IntoView {
    let photo = || Some(format!("{}/photo.png", dir()));
    let portrait = || Some(format!("{}/portrait.png", dir()));
    view! {
        column(class = "gallery") {
            row(class = "row") {
                Panel(title = "thumbnail") {
                    image(class = "thumb", src = photo())
                }
                Panel(title = "medium") {
                    image(class = "mid", src = photo())
                }
            }
            row(class = "row") {
                Panel(title = "cover") {
                    image(class = "fit fit--cover", src = portrait())
                }
                Panel(title = "contain") {
                    image(class = "fit fit--contain", src = portrait())
                }
                Panel(title = "none") {
                    image(class = "fit fit--none", src = portrait())
                }
                Panel(title = "fill") {
                    image(class = "fit", src = portrait())
                }
            }
        }
    }
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #14161c;
        font-family: sans-serif;
        padding: 24px;
    }

    .gallery { display: flex; flex-direction: column; gap: 20px; }

    .row { display: flex; gap: 20px; align-items: flex-start; }

    .panel { display: flex; flex-direction: column; gap: 6px; }

    .panel__title {
        color: #aeb4c2;
        font-size: 12px;
        font-family: sans-serif;
    }

    .thumb { width: 96px; height: 64px; border-radius: 8px; }

    .mid { width: 480px; height: 320px; border-radius: 12px; }

    .fit {
        width: 150px;
        height: 150px;
        border-radius: 20px;
        background-color: #232734;
    }

    .fit--cover { object-fit: cover; }
    .fit--contain { object-fit: contain; }
    .fit--none { object-fit: none; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Images")
        .with_title("Images")
        .with_size(760.0, 640.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Gallery() })
}
