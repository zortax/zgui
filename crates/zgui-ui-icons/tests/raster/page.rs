//! The documents the pixels are measured on, and the runs that produce them.
//!
//! Three documents, because the questions divide into three: an icon alone on a flat panel, the
//! same panel with no drawing on it at all, and an icon with an opaque box drawn after it.
//!
//! The panel covers the whole window on purpose. It makes "everything outside the drawing" one flat
//! colour, which is what lets a scissored repaint be compared to a full one pixel for pixel instead
//! of only inside the rectangle it promised to redraw.

use std::sync::{Arc, Mutex, OnceLock};

use zgui::platform::{AppHandler, PlatformError};
use zgui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::status::INFO;

use crate::raster::device::{self, Log};
use crate::raster::script::Recorded;

/// How wide and tall the window is, in device pixels at a ratio of one.
pub const SURFACE: i32 = 160;

/// The padding the panel puts around the drawing, in CSS pixels.
pub const PADDING: f32 = 16.0;

/// The side of the box the drawing is fitted into.
pub const ICON_BOX: f32 = 96.0;

/// The side of the square the outlines are written in, which is the icon's own view box.
pub const VIEW_BOX: f32 = 24.0;

/// The panel behind the drawing, which is also every pixel the drawing does not cover.
pub const PANEL: [u8; 4] = [240, 200, 40, 255];

/// The colour the drawing takes, inherited from the text colour around it and declared nowhere on
/// the element itself.
pub const INK: [u8; 4] = [0, 96, 224, 255];

/// The box drawn after the drawing in the layered document.
pub const LID: [u8; 4] = [0, 0, 0, 255];

/// Where the lid starts, in device pixels from the top of the window.
pub const LID_TOP: i32 = 64;

/// The centre of the drawing's view box, in device pixels.
pub fn centre() -> (f64, f64) {
    let origin = f64::from(PADDING);
    let half = f64::from(ICON_BOX) / 2.0;
    (origin + half, origin + half)
}

/// How many device pixels one unit of the icon's own square becomes.
pub fn factor() -> f64 {
    f64::from(ICON_BOX) / f64::from(VIEW_BOX)
}

/// The sheet the panel and the drawing's size come from.
///
/// The drawing's colour is `color` and nothing else: the component declares no fill, so an icon
/// takes the colour of the text around it. `--zui-icon-xl` is the token the component's own sheet
/// reaches through, set here to a size large enough that an edge has room to be an edge.
const SHEET: &str = ":root { background-color: rgb(240, 200, 40); color: rgb(0, 96, 224);
                             --zui-icon-xl: 96px }
     .frame { display: flex; flex-direction: column; align-items: flex-start;
              width: 128px; height: 128px; padding: 16px;
              background-color: rgb(240, 200, 40) }
     .lid { width: 96px; height: 48px; margin-top: -48px; background-color: rgb(0, 0, 0) }";

/// A panel with one icon on it.
#[component]
fn Marked() -> impl IntoView {
    view! {
        box(class = "frame") {
            Icon(icon = INFO, size = IconSize::Xl)
        }
    }
}

/// The same panel with nothing drawn on it.
#[component]
fn Bare() -> impl IntoView {
    view! { box(class = "frame") }
}

/// The same icon with an opaque box drawn after it, covering its lower half.
#[component]
fn Layered() -> impl IntoView {
    view! {
        box(class = "frame") {
            Icon(icon = INFO, size = IconSize::Xl)
            box(class = "lid")
        }
    }
}

/// Drives the application over buffers until it settles, and stops.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(8);
    harness.shut_down();
    Ok(())
}

/// Every frame of the three runs.
pub struct Runs {
    /// The panel with an icon on it.
    pub marked: Vec<Recorded>,
    /// The panel with nothing on it.
    pub bare: Vec<Recorded>,
    /// The icon with a box drawn after it.
    pub layered: Vec<Recorded>,
}

impl Runs {
    /// The last frame of a run, which is the settled one.
    pub fn settled(frames: &[Recorded]) -> &Recorded {
        frames
            .last()
            .expect("the application drew at least one frame")
    }
}

/// The three runs, performed once however many assertions read them.
///
/// `None` when this machine has no graphics device, which every caller says out loud when it skips.
pub fn runs() -> Option<&'static Runs> {
    static RUNS: OnceLock<Option<Runs>> = OnceLock::new();
    RUNS.get_or_init(|| {
        if !device::available() {
            return None;
        }
        let _guard = device::device_lock();
        Some(Runs {
            marked: run("marked", || view! { Marked() }),
            bare: run("bare", || view! { Bare() }),
            layered: run("layered", || view! { Layered() }),
        })
    })
    .as_ref()
}

/// Runs one document to a standstill and hands back what each of its frames drew.
fn run<V: IntoView + 'static>(title: &str, view: impl FnMut() -> V + 'static) -> Vec<Recorded> {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    zgui::app()
        .with_title(title)
        .with_size(SURFACE as f32, SURFACE as f32)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(device::factory(&log, PANEL)))
        .run_on(buffers, view)
        .expect("the application ran");
    let frames = core::mem::take(&mut *log.lock().unwrap_or_else(|held| held.into_inner()));
    assert!(
        !frames.is_empty(),
        "the `{title}` document drew no frame at all"
    );
    frames
}
