//! A modal dialog standing open over content that never stops moving, on a bare Linux console.
//!
//! The scene the frosted scrim is measured against. A `backdrop-filter` over the whole window is
//! the one composite that reads far more than it writes, and what it costs depends entirely on
//! whether the copy it filters has to be made afresh every frame. Nothing here needs a keyboard:
//! the dialog is open from the first frame and the spinners turn on their own, which is exactly
//! the arrangement a person reports as "the modal makes it crawl".
//!
//! ```text
//! cargo build -p zgui-ui --release --example tty_scrim --features drm
//! ZGUI_LATENCY=/tmp/scrim.jsonl ./tty_scrim
//! ```
//!
//! **Escape leaves**, for the same reason the gallery binds it: a console application that binds no
//! way out has to be killed from another terminal, and a kill runs no destructor, so the console is
//! left in graphics mode with its keyboard turned off.

use zgui::prelude::*;
// `run_drm` is a trait method here: the console is a crate of its own rather than a feature of
// the framework, so a program that wants the kernel's display names it.
use zgui::reactive::{RenderEffect, RwSignal};
use zgui::{component, view};
use zgui_console::RunDrm;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

/// How many spinners turn behind the scrim, which is how many rectangles a frame damages.
fn spinners() -> usize {
    std::env::var("ZGUI_SCRIM_SPINNERS")
        .ok()
        .and_then(|it| it.parse().ok())
        .unwrap_or(6)
}

/// The page's own layout, in tokens, exactly as the gallery's shell does it.
const SHEET: &str = zgui::css!(
    ":root {
        background-color: var(--zui-color-background);
        color: var(--zui-color-foreground);
        font-family: sans-serif;
        font-size: var(--zui-type-size-md);
        overflow: auto;
    }"
);

/// The content the scrim is over: a page of panels, several of them animating.
#[component]
fn Beneath() -> impl IntoView {
    view! {
        column(style = "padding: 32px; gap: 24px; background: #f4f4f5") {
            text(style = "font-size: 28px; font-weight: 700") {"Everything under the scrim"}
            row(style = "gap: 24px; flex-wrap: wrap") {
                {(0..spinners())
                    .map(|index| {
                        view! {
                            column(style = "gap: 8px; padding: 16px; background: #ffffff;
                                            border: 1px solid #e4e4e7; border-radius: 8px") {
                                Spinner()
                                text {{format!("Panel {index}")}}
                            }
                        }
                    })
                    .collect::<Vec<_>>()}
            }
            {(0..6)
                .map(|index| {
                    view! {
                        text(style = "font-size: 15px") {
                            {format!(
                                "Row {index}: text under the scrim, so that the blur has \
                                 something with edges in it to carry."
                            )}
                        }
                    }
                })
                .collect::<Vec<_>>()}
        }
    }
}

/// The scene: the page, a dialog standing open over it, and the one binding a console needs.
#[component]
fn Scene() -> impl IntoView {
    let anchor = NodeRef::new();
    let registration = RenderEffect::new(move |previous: Option<Option<WindowShortcut>>| {
        drop(previous);
        anchor.get();
        anchor.window_shortcut()
    });
    on_cleanup_local(move || drop(registration));
    let windows = use_windows();

    // The comparison the measurement is: the same page with the scrim over it and without.
    let modal = std::env::var_os("ZGUI_SCRIM_CLOSED").is_none();
    let scheme = RwSignal::new_local(ColorScheme::Light);
    let light = RwSignal::new_local(Preset::default());
    let dark = RwSignal::new_local(Preset::default());

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
            ThemeProvider(
                scheme = scheme,
                light = Signal::derive_local(move || light.get().light()),
                dark = Signal::derive_local(move || dark.get().dark())
            ) {
            // The block comes first because a component call followed by one reads as that
            // component's children. The surface is portalled onto an overlay band either way, so
            // where it is written is not where it is drawn.
            {modal.then(|| {
                AnyView::new(view! {
                        Dialog(default_open = true) {
                            DialogContent {
                                DialogHeader {
                                    DialogTitle {"Rename project"}
                                    DialogDescription {
                                        "Everyone on the team will see the new name."
                                    }
                                }
                                DialogFooter {
                                    Button {"Rename"}
                                }
                            }
                        }
                })
            })}
            Beneath()
            }
        }
    }
}

/// Sends the log to a file, because on this console standard error is the screen.
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
        .finish();
    drop(tracing::subscriber::set_global_default(subscriber));
}

/// Opens the scene on the console.
fn main() -> Result<(), zgui::Error> {
    log();
    let described = app()
        .with_application_id("dev.zgui.ScrimConsole")
        .with_title("zgui scrim")
        .with_size(1280.0, 1024.0)
        .with_stylesheet(SHEET);
    if std::env::var_os("ZGUI_TTY_WINDOWED").is_some() {
        return described.run(|| view! { Scene() });
    }
    described.run_drm(|| view! { Scene() })
}
