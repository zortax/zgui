//! A whole application, assembled the way an application assembles one, driven with no window.
//!
//! What this is really asserting is that the umbrella's wiring is *connected*. An application
//! written against this crate names a view and a style sheet and nothing else, which means every
//! other decision — which font engine shapes the text, which rasteriser turns its glyphs into
//! pixels, what answers the cascade's font-metric questions, what draws a window — is taken here.
//! A decision taken and not wired up produces a program that opens, runs its whole frame pipeline
//! and puts nothing on the screen, so the assertion is on what the window draws: a background,
//! and glyphs.

use zgui::platform::{AppHandler, PlatformError};
use zgui::prelude::*;
use zgui_testkit_compositor::Screen;

/// A view with one styled box and one run of text in it.
#[component]
fn Panel() -> impl IntoView {
    let (count, set_count) = signal(0);
    view! {
        column(class = "panel") {
            text(class = "panel__label") {{move || format!("count {}", count.get())}}
            control(class = "panel__button", on:click = move |_| set_count.update(|n| *n += 1)) {
                "+"
            }
        }
    }
}

/// The sheet the window is styled by.
const SHEET: &str = css!(
    ":root { background-color: #101216; color: #ffffff; font-family: sans-serif }
     .panel { padding: 16px; gap: 8px; background-color: #202634; border-radius: 8px }
     .panel__label { font-size: 18px }"
);

/// Drives the application over buffers for a few turns and stops.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(4);
    harness.shut_down();
    Ok(())
}

#[test]
fn an_application_built_through_the_umbrella_draws_its_boxes_and_its_glyphs() {
    // The screen stands in for the graphics device. The document, the cascade, layout, shaping,
    // the paint stage and the compositor above it are the real thing.
    let screen = Screen::new();
    let shown = screen.shown();
    let mut screen = Some(screen);
    app()
        .with_title("umbrella")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_compositor_client(Box::new(zgui_testkit_compositor::driver))
        .with_compositor_renderer(Box::new(move |_size| {
            Box::new(screen.take().expect("the application opens one window"))
                as Box<dyn compositor_api::Renderer>
        }))
        .run_on(buffers, || view! { Panel() })
        .expect("the application ran");

    let drawing = shown.drawing().expect("no frame was drawn at all");
    assert!(
        !drawing.quads().is_empty(),
        "the styled boxes were not drawn:\n{drawing}"
    );
    assert!(
        drawing.glyphs().len() >= 5,
        "the text was laid out and no glyph was drawn: {} sprites\n{drawing}",
        drawing.glyphs().len()
    );
}
