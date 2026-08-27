//! What a long scroll through a virtualised list leaves in the clip table.
//!
//! Every recycled row interns chains of its own — a box chain named by the row's label, a cut
//! clip minted where its ellipsis falls — and the row that scrolls away leaves them behind. The
//! runtime sweeps the table on a stride of frames, so a session that scrolled a large document
//! end to end settles back to what is on screen instead of paying for every position the window
//! ever held.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

#![forbid(unsafe_code)]

use std::time::Duration;

use zgui::app::Fonts;
use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
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

/// How many rows the list holds — far more than the churn visits, so the fling never runs out.
const ROWS: usize = 20_000;

/// How tall one row is.
const ROW: f32 = 17.0;

/// One tick of a 120 Hz refresh.
const TICK: Duration = Duration::from_micros(8_333);

/// Rows whose one label always overflows, so every recycled row mints a cut clip of its own.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .list { height: 590px }
     .row { flex-direction: row; height: 17px; align-items: center }
     .beat { height: 10px; color: #5b6272 }
     .summary { flex: 1 1 auto; white-space: nowrap; overflow: hidden;
                text-overflow: ellipsis }"
);

/// The list under a one-line heartbeat, which is what keeps frames coming while the table ages.
#[component]
fn Fixture(
    /// The heartbeat's reading.
    beat: zgui::reactive::RwSignal<usize, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    view! {
        column {
            label(class = "beat") {{move || beat.get().to_string()}}
            VirtualList(
                count = ROWS,
                row_size = ROW,
                label = "Rows",
                class = "list",
                row = move |index: usize| view! {
                    row(class = "row") {
                        label(class = "summary") {{format!(
                            "row {index} runs on far past the right edge of the port, so the \
                             ellipsis always cuts it and every position mints a clip"
                        )}}
                    }
                }
            )
        }
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
fn opened(
    beat: zgui::reactive::RwSignal<usize, zgui::reactive::LocalStorage>,
) -> Harness<Runtime> {
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("clip-sweep")
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
            Box::new(view! { Fixture(beat = beat) }.into_view().build(cx))
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
fn a_scrolled_past_document_is_swept_out_of_the_clip_table() {
    let beat = zgui::reactive::RwSignal::new_local(0_usize);
    let mut harness = opened(beat);
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

    // A long fling: a ten-line notch per tick carries the window across ~2,000 rows, every one
    // of them recycled in and out.
    for _ in 0..400 {
        harness.deliver_to_first(notch(10.0));
        harness.advance(TICK);
        harness.pump();
    }
    harness.settle(64);
    let peak = counter::get(Counter::ClipEntriesLive);
    assert!(
        peak > 2_000,
        "the fling left {peak} live chains, too few for the fixture to mean anything"
    );

    // The table ages while frames keep coming: the heartbeat is the only thing changing, the
    // scrolled-past chains fall behind the keep horizon, and the stride's next sweeps take them.
    for tick in 0..400 {
        beat.set(tick + 1);
        harness.settle(16);
        harness.advance(Duration::from_millis(8));
        harness.pump();
    }
    let settled = counter::get(Counter::ClipEntriesLive);
    assert!(
        settled < peak / 4,
        "{peak} chains at the fling's end were still {settled} after the table aged"
    );
    assert!(
        settled < 1_500,
        "{settled} live chains for one port of rows is not a swept table"
    );

    // What the sweep left must still scroll: the way back re-interns what it needs and nothing
    // resolves through anything freed.
    for _ in 0..200 {
        harness.deliver_to_first(notch(-12.0));
        harness.advance(TICK);
        harness.pump();
    }
    harness.settle(64);
}
