//! Every component in the library, in one window.
//!
//! Run it with `cargo run -p zgui-ui --example gallery`.
//!
//! This is an ordinary downstream program: it imports [`zgui::prelude`], the component library's
//! own prelude, the design tokens and the icons, and reaches for nothing else. If something here
//! needed a framework internal to work, an application would need the same internal, so the
//! gallery is also the test of whether the public surface is enough.
//!
//! The switch in the masthead writes one signal. That signal is the [`ThemeProvider`]'s scheme, and
//! the provider writes the tokens out as custom properties on `:root` — so flipping it re-colours
//! the whole window, including the surfaces portalled onto the overlay band, without a single
//! component being told about it.

mod app;
mod section;
mod shell;

use zgui::prelude::*;
use zgui::view;

use crate::app::GalleryProps;

/// Opens the window.
fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Gallery")
        .with_title("zgui components")
        .with_size(crate::app::WIDTH, crate::app::HEIGHT)
        .with_stylesheet(crate::shell::SHEET)
        .run(|| view! { Gallery() })
}
