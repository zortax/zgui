//! The component gallery, opened as a real window and driven through every panel.
//!
//! This is the gallery beside it, in the same window, with the same style sheet and the same
//! components — the view is that example's own module, so what is driven cannot drift away from
//! what is shipped. What is added is a handler between the desktop and the application, which
//! delivers pointer, keyboard and wheel events, runs the frames they ask for, reads back where
//! everything ended up and what it says, and captures the window between steps.
//!
//! It is a downstream program in the same sense the gallery is: it names `zgui`, the component
//! library, the tokens and the icons, and nothing else. Anything it needs in order to drive itself
//! is therefore something any application could use to drive itself, which is the point of running
//! it from here rather than from inside the framework.
//!
//! ```text
//! ZGUI_PROBE_APPID=zgui-gal-1234 ZGUI_PROBE_SHOTS=/tmp/shots \
//!     cargo run -p zgui-ui --release --example gallery_probe
//! ```

#[path = "../gallery/app.rs"]
mod app;
mod driver;
mod report;
#[path = "../gallery/section/mod.rs"]
mod section;
#[path = "../gallery/shell.rs"]
mod shell;

mod script;
mod stage;

use std::path::PathBuf;

use zgui::prelude::*;
use zgui::view;

use crate::app::GalleryProps;
use crate::report::Report;
use crate::stage::handles::Grab;

/// The environment variable naming the file the findings go to.
const REPORT: &str = "ZGUI_PROBE_REPORT";

/// Opens the window and drives it.
fn main() -> Result<(), zgui::Error> {
    let id = std::env::var(crate::stage::shot::APP_ID).unwrap_or_else(|_| "zgui-gal".to_owned());
    let report = Report::new(PathBuf::from(
        std::env::var(REPORT).unwrap_or_else(|_| "target/probe-report.tsv".to_owned()),
    ));

    app()
        .with_application_id(id.as_str())
        .with_title(id.as_str())
        .with_size(crate::app::WIDTH, crate::app::HEIGHT)
        .with_stylesheet(crate::shell::SHEET)
        .run_on(
            move |inner| zgui::app::desktop()(Box::new(crate::driver::Probe::new(inner, report))),
            || (Grab, view! { Gallery() }.into_view()),
        )
}
