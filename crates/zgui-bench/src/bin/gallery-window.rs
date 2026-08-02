//! The shipped gallery in a real window, under an application id a diagnostic can name.
//!
//! The gallery's own binary opens under `dev.zgui.Gallery`, which is right for the gallery and
//! wrong for a measurement: a session that injects input has to be able to prove the window it is
//! about to type into is its own, and it can only do that if the window carries an identifier
//! nothing else on the desktop shares. So this opens the same component — the very `app` module the
//! gallery ships, included through `#[path]` — under an id given on the command line.
//!
//! ```text
//! ZGUI_LATENCY=/tmp/gallery.jsonl gallery-window dev.zgui.diag-1234
//! ```

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    reason = "the gallery's own source is included whole, and this opens the window rather than \
              every part the sections are built from"
)]

#[path = "../../../zgui-ui/examples/gallery/app.rs"]
mod app;
#[path = "../../../zgui-ui/examples/gallery/section/mod.rs"]
#[allow(
    unused_imports,
    reason = "the gallery's sections are one module; the ladder below mounts the ones it is sized by"
)]
mod section;
#[path = "../../../zgui-ui/examples/gallery/shell.rs"]
mod shell;

use zgui::prelude::*;
use zgui::view;

use crate::app::GalleryProps;

/// Opens the window under the application id named by the first argument.
fn main() -> Result<(), zgui::Error> {
    let id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dev.zgui.gallery-window".to_owned());
    app()
        .with_application_id(id.clone())
        .with_title(id)
        .with_size(crate::app::WIDTH, crate::app::HEIGHT)
        .with_stylesheet(crate::shell::SHEET)
        .run(|| view! { Gallery() })
}
