//! What the application does between an input event arriving and a frame being presented.
//!
//! This opens one of the shipped examples as an ordinary desktop program and records instants at
//! two seams it does not otherwise cross: the moment the windowing backend hands an event to the
//! framework, and the two ends of the device work that acquires, submits and presents a frame.
//! Nothing in between is instrumented, on purpose — a gap that nobody named is still visible as a
//! gap.
//!
//! `ZGUI_FEEL_TAPE` names the file the recording goes to; it is rewritten every time the loop is
//! about to park, so a run killed from outside still leaves a complete file. `ZGUI_FEEL_VIEW`
//! chooses `gallery` (the default), `counter`, `todo` — the interface with a text field, and therefore the
//! one a keystroke can be measured through — or `rulers`, which is lines of one repeated letter
//! for measuring where glyphs land. `ZGUI_FEEL_SHOT` names a file the first presented frame is
//! read back into.
//!
//! Run it with
//! `ZGUI_FEEL_TAPE=/tmp/tape.jsonl ZGUI_DIAG_APPID=some-id cargo run --release -p zgui --example feel`.

mod counter;
mod gallery;
mod handler;
mod render;
mod ruler;
mod tape;
mod todo;

use std::path::PathBuf;
use std::sync::Arc;

use zgui::prelude::*;
use zgui_platform::{AppHandler, PlatformError, Surface};
use zgui_render::RenderTarget;

use crate::counter::CounterProps;
use crate::gallery::GalleryProps;
use crate::ruler::RulersProps;
use crate::todo::TodosProps;

/// Opens the window, runs the loop, and writes the recording out.
fn main() -> Result<(), zgui::Error> {
    let out = PathBuf::from(
        std::env::var("ZGUI_FEEL_TAPE").unwrap_or_else(|_| "target/feel.jsonl".to_owned()),
    );
    let tape = tape::Tape::new(out);
    let id = std::env::var("ZGUI_DIAG_APPID").unwrap_or_else(|_| "zgui-diag-feel".to_owned());
    let view = std::env::var("ZGUI_FEEL_VIEW").unwrap_or_default();

    let factory = {
        let tape = tape.clone();
        Box::new(move |surface: &Arc<dyn Surface>, target: RenderTarget| {
            render::build(surface, target, tape.clone())
        })
    };

    let driver = {
        let tape = tape.clone();
        move |inner: Box<dyn AppHandler>| -> Result<(), PlatformError> {
            let wrapped = handler::Timed::new(inner, tape.clone());
            let result = zgui_platform_winit::run(Box::new(wrapped));
            tape.borrow_mut().write();
            result
        }
    };

    let app = app()
        .with_application_id(id.as_str())
        .with_title(id.as_str())
        .with_renderer(factory);
    match view.as_str() {
        "todo" => app
            .with_size(520.0, 520.0)
            .with_stylesheet(todo::SHEET)
            .run_on(driver, || view! { Todos() }),
        "counter" => app
            .with_size(360.0, 260.0)
            .with_stylesheet(counter::SHEET)
            .run_on(driver, || view! { Counter() }),
        "rulers" => app
            .with_size(1400.0, 800.0)
            .with_stylesheet(ruler::SHEET)
            .run_on(driver, || view! { Rulers() }),
        _ => app
            .with_size(1080.0, 720.0)
            .with_stylesheet(gallery::SHEET)
            .run_on(driver, || view! { Gallery() }),
    }
}
