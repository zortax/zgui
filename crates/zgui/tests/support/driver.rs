//! Driving the gallery: opening it, and scripting the interactions a budget is written about.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use zgui::prelude::*;
use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

use crate::support::fixture::{PageProps, SHEET};

/// How wide the surface every case opens is, in device pixels.
const WIDTH: f32 = 1080.0;

/// How tall it is.
const HEIGHT: f32 = 720.0;

/// The surface every case opens.
fn surface() -> Size<DevicePx, Device> {
    Size::new(DevicePx(WIDTH), DevicePx(HEIGHT))
}

/// How many turns a delivered event is given to settle into a finished frame.
const TURNS: u32 = 32;

/// The gallery, open and driveable.
pub struct Gallery {
    /// The application, over the headless platform.
    harness: Harness<Runtime>,
    /// Where the swatches are, in CSS pixels, left to right.
    swatches: Vec<Point<CssPx, Css>>,
    /// The frame count when the last measurement began.
    baseline: u64,
    /// Held for the life of the fixture: the counters are one block for the whole process.
    _turn: MutexGuard<'static, ()>,
}

impl Gallery {
    /// How many swatches the document has.
    pub const SWATCHES: usize = 4;

    /// Opens the gallery over a surface of the standard extent, with a real font engine.
    ///
    /// # Panics
    ///
    /// Panics when the document does not produce the swatches the script clicks, which would leave
    /// every case measuring an interaction with nothing.
    pub fn open() -> Self {
        let turn = exclusive();
        let fonts = Fonts::system();
        let metrics = fonts.clone();
        let shaping = fonts.clone();
        let raster = fonts.clone();
        let runtime: Runtime = zgui_runtime::App::new()
            .with_title("wall-clock")
            .with_size(1080.0, 720.0)
            .with_stylesheet(SHEET)
            .with_renderer(Box::new(crate::support::renderer::build))
            .with_metrics(Box::new(move || metrics.metrics()))
            .with_text_engine(Box::new(move || {
                Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
            }))
            .with_glyph_raster(Box::new(move || raster.raster()))
            .into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
                Box::new(view! { Page() }.into_view().build(cx))
            })
            .expect("the reactive runtime installs");
        let mut harness = Harness::new(runtime);
        harness.deliver_to_first(SurfaceEvent::Resized(surface()));
        harness.settle(64);
        let swatches = swatch_centres(&harness.app().windows()[0]);
        assert_eq!(
            swatches.len(),
            Self::SWATCHES,
            "the fixture did not produce the swatches every case clicks"
        );
        Self {
            harness,
            swatches,
            baseline: 0,
            _turn: turn,
        }
    }

    /// Runs the loop until nothing is owed, and takes that as the baseline for what follows.
    pub fn settle(&mut self) {
        self.harness.settle(64);
        self.baseline = self.harness.frames_requested();
    }

    /// How many frames have run since the last [`Gallery::settle`].
    pub fn frames(&self) -> u64 {
        self.harness.frames_requested() - self.baseline
    }

    /// Moves the pointer onto one swatch, presses and releases, settling each event.
    pub fn click_swatch(&mut self, index: usize) {
        let at = self.swatches[index];
        self.deliver(PointerAction::Moved, at);
        self.deliver(PointerAction::Pressed, at);
        self.deliver(PointerAction::Released, at);
    }

    /// Moves the pointer onto one swatch.
    pub fn hover_swatch(&mut self, index: usize) {
        let at = self.swatches[index];
        self.deliver(PointerAction::Moved, at);
    }

    /// Moves the pointer off every swatch, into the corner of the window.
    pub fn hover_away(&mut self) {
        self.deliver(PointerAction::Moved, Point::new(CssPx(4.0), CssPx(4.0)));
    }

    /// Widens the surface by one step of a drag.
    pub fn resize_step(&mut self, step: usize) {
        let width = WIDTH + (step % 24) as f32 * 8.0;
        self.harness
            .deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(width),
                DevicePx(HEIGHT),
            )));
        self.harness.settle(TURNS);
    }

    /// Advances the clock by one refresh interval with nothing to do.
    pub fn idle_tick(&mut self) {
        self.harness.advance(Duration::from_millis(16));
        self.harness.pump();
    }

    /// Closes the window.
    pub fn shut_down(&mut self) {
        self.harness.shut_down();
    }

    /// Delivers one pointer event and runs the loop until the frame it asked for has finished.
    fn deliver(&mut self, action: PointerAction, at: Point<CssPx, Css>) {
        self.harness.deliver_to_first(SurfaceEvent::Pointer {
            action,
            event: PointerEvent::mouse(at),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        self.harness.settle(TURNS);
    }
}

/// The counter block is one set of numbers for the whole process, so fixtures take turns.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The centre of every 34 by 34 box in the window, which is what the swatches are.
///
/// Read out of the fragment tree rather than computed from the sheet: a constant here would be
/// asserting the test's own arithmetic, and a script that clicks the wrong place measures a
/// document nothing happened to.
fn swatch_centres(window: &zgui_runtime::Window) -> Vec<Point<CssPx, Css>> {
    let layout = window.layout().borrow();
    let mut found = Vec::new();
    for key in layout.keys() {
        for fragment in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*fragment) else {
                continue;
            };
            let border = fragment.border_box;
            let width = border.size.width.0;
            let height = border.size.height.0;
            if (width - 34.0).abs() < 0.5 && (height - 34.0).abs() < 0.5 {
                found.push(Point::new(
                    CssPx(border.origin.x.0 + width / 2.0),
                    CssPx(border.origin.y.0 + height / 2.0),
                ));
            }
        }
    }
    found.sort_by(|a, b| a.x.0.total_cmp(&b.x.0));
    found
}
