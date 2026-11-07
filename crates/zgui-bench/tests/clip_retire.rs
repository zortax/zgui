//! What rows that remount leave in the clip table while they keep remounting.
//!
//! A clipped box names a chain, so a list that replaces its rows on every frame adds a chain per
//! row per frame. The table has to stay in proportion to the rows on screen during the churn, with
//! no quiet frames for a sweep to age anything in.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

#![forbid(unsafe_code)]

use std::time::Duration;

use zgui::app::Fonts;
use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx, IntoView, View};
use zgui::vocab::{
    Modifiers, PointerAction, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase,
    Timestamp, WheelEvent,
};
use zgui::{component, view};
use zgui_platform_headless::Harness;
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_ui::prelude::*;

/// How wide the window opens, in CSS pixels.
const WIDTH: f32 = 900.0;

/// How tall it opens, which is also the port.
const HEIGHT: f32 = 600.0;

/// How many rows the list holds, far more than the churn visits.
const ROWS: usize = 20_000;

/// How tall one row is.
const ROW: f32 = 17.0;

/// One tick of a 120 Hz refresh.
const TICK: Duration = Duration::from_micros(8_333);

/// How many clip slots one port of rows may take, with room for the rows being retired.
const MOST_SLOTS: u64 = 600;

/// Rows with one clipped cell each, and text short enough to mint no cut clip.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .list { height: 590px }
     .row { flex-direction: row; height: 17px; align-items: center }
     .cell { flex: 1 1 auto; white-space: nowrap; overflow: hidden }"
);

/// The list.
#[component]
fn Fixture() -> impl IntoView {
    view! {
        VirtualList(
            count = ROWS,
            row_size = ROW,
            label = "Rows",
            class = "list",
            row = move |index: usize| view! {
                row(class = "row") {
                    label(class = "cell") {{format!("row {index}")}}
                }
            }
        )
    }
}

/// The middle of the port, which is where the wheel points.
fn middle() -> Point<CssPx, Css> {
    Point::new(CssPx(WIDTH / 2.0), CssPx(HEIGHT / 2.0))
}

/// One wheel notch of `lines`.
fn notch(lines: f32) -> SurfaceEvent {
    SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
            position: middle(),
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// Opens the window over the machine's own fonts.
fn opened() -> Harness<Runtime> {
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("clip-retire")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(
            |_surface: &std::sync::Arc<dyn zgui::platform::Surface>,
             target: RenderTarget|
             -> Result<Box<dyn Renderer>, AppError> {
                let mut inner = zgui_testkit_scene::CaptureRenderer::new();
                inner.configure(target);
                Ok(Box::new(inner))
            },
        ))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Fixture() }.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(WIDTH),
        DevicePx(HEIGHT),
    )));
    harness.settle(512);
    harness
}

#[test]
fn rows_that_keep_remounting_keep_the_clip_table_small() {
    let mut harness = opened();
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(middle()),
        modifiers: Modifiers::default(),
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(64);
    if !COUNTERS_ENABLED {
        return;
    }

    let mut peak = 0;
    for _ in 0..400 {
        harness.deliver_to_first(notch(10.0));
        harness.advance(TICK);
        harness.pump();
        peak = peak.max(counter::get(Counter::ClipSlotsReach));
    }
    assert!(
        peak > 0,
        "the fixture never reached the table, so this says nothing"
    );
    assert!(
        peak <= MOST_SLOTS,
        "the churn reached {peak} clip slots for one port of rows"
    );

    // What the passes left must still scroll: nothing resolves through a freed chain.
    for _ in 0..200 {
        harness.deliver_to_first(notch(-12.0));
        harness.advance(TICK);
        harness.pump();
    }
    harness.settle(64);
}
