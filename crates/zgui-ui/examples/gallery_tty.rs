//! The component gallery, on a bare Linux console.
//!
//! The same view the windowed gallery opens, through the console backend instead of a desktop —
//! the view is that example's own module, so what runs here cannot drift away from what is
//! shipped. That is the whole purpose: a component that behaves one way in a window and another
//! way on a console has a fault in the console backend, and one that misbehaves in both has a
//! fault in the framework.
//!
//! **This one needs the device.** Switch to a spare terminal, log in, and run:
//!
//! ```text
//! cargo build -p zgui-ui --release --example gallery_tty --features drm
//! ./target/release/examples/gallery_tty
//! ```
//!
//! It takes the keyboard away from everything else, so `Ctrl+C` raises no `SIGINT`. **Escape
//! leaves**, bound in a wrapper below for that reason: a console application that binds no way out
//! has to be killed from another terminal, and a kill runs no destructor, so the console is left in
//! graphics mode with its keyboard turned off.
//!
//! The window size the gallery asks for is what a desktop would open; a console has whatever the
//! display is and the request is ignored. The gallery is laid out for 1600 by 1000 CSS pixels, so
//! a smaller display shows part of it and scrolls the rest, which is itself worth looking at.

#[path = "gallery/app.rs"]
mod app;
#[path = "gallery/section/mod.rs"]
mod section;
#[path = "gallery/shell.rs"]
mod shell;

use zgui::prelude::*;
// `run_drm` is a trait method here: the console is a crate of its own rather than a feature of
// the framework, so a program that wants the kernel's display names it.
use zgui::reactive::RenderEffect;
use zgui::{component, view};
use zgui_console::RunDrm;

use crate::app::GalleryProps;

/// The gallery, with the one binding a console application cannot do without.
///
/// A wrapper rather than a change to the gallery's own module, because the two want different
/// things from the same key: on a console Escape is the only way out, and in a window it dismisses
/// whichever menu, dialog or drawer is open. Binding it inside the shared component would take that
/// away from the windowed gallery.
///
/// It is `display: contents`, so it generates no box and the gallery lays out exactly as it does
/// under a desktop. What it contributes is a node to hang a window shortcut on — a key reaches a
/// window in which nothing has focus only through one of those.
#[component]
fn Console() -> impl IntoView {
    let anchor = NodeRef::new();
    let registration = RenderEffect::new(move |previous: Option<Option<WindowShortcut>>| {
        drop(previous);
        anchor.get();
        anchor.window_shortcut()
    });
    on_cleanup_local(move || drop(registration));
    let windows = use_windows();

    view! {
        column(
            node_ref = anchor,
            style = "display: contents",
            on:key_down = move |ev| {
                if matches!(&ev.key, Key::Named(NamedKey::Escape)) {
                    windows.quit();
                }
            },
        ) {
            Gallery()
        }
    }
}

/// Sends the log to a file, because on this console standard error is the screen.
///
/// The same arrangement the clock example makes, and for the same reason: a message written to
/// standard error lands on pixels the frame loop is about to overwrite. `ZGUI_TTY_LOG` names the
/// file. A closing span carries how long it was open, which is what makes a frame's cost readable
/// here at all — a component gallery on a slow machine is the case where that matters most.
fn log() {
    let Ok(path) = std::env::var("ZGUI_TTY_LOG") else {
        return;
    };
    let Ok(file) = std::fs::File::create(&path) else {
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .finish();
    drop(tracing::subscriber::set_global_default(subscriber));
}

/// Opens the gallery on the console.
fn main() -> Result<(), zgui::Error> {
    log();
    let described = app()
        .with_application_id("dev.zgui.GalleryConsole")
        .with_title("zgui components")
        .with_size(crate::app::WIDTH, crate::app::HEIGHT)
        .with_stylesheet(crate::shell::SHEET);

    // The same application in a window, for telling this backend's faults from the framework's,
    // exactly as the clock example offers.
    if std::env::var_os("ZGUI_TTY_WINDOWED").is_some() {
        return described.run(|| view! { Console() });
    }
    described.run_drm(|| view! { Console() })
}
