//! A diagnostic probe: the same list with every row mounted, and the slope that produces.
//!
//! **This is not a CI reference workload and must not become one.** It has no criterion, it gates
//! nothing, and `cargo xtask` does not run it. What it is for is stated here rather than left to be
//! inferred, because the reason it exists is the same reason it must stay out of the gates.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin unvirtualised-probe
//! ```
//!
//! # Why it is a probe and not a workload
//!
//! It is the only document in this repository that would make the scroll phase's slope look
//! important. Everything the compositor programme might do to scrolling — a scroll tree, resolve-
//! time culling, cached pictures — is worth a great deal on a document whose every row exists and
//! is composed on every frame, and worth much less on a list that only ever mounts the rows in
//! front of the port. Wiring this into `ci` would let a phase justify itself against a document no
//! application would ship: the number would be real, the improvement would be real, and the
//! conclusion drawn from it would be false.
//!
//! The list beside it — [`zgui_ui::virtualize::VirtualList`], measured by `list-slope` — is what an
//! application that shows a hundred thousand rows actually builds, and that is the document the
//! gates compare against. This one characterises **a slope nobody is expected to hit**, and it is
//! run by a person who is deciding something, on purpose, once.
//!
//! # What it measures
//!
//! The same rows, the same sheet and the same wheel gesture as `list-slope`, with the virtualiser
//! taken out: every row is an element. Four sizes, and the least-squares slope of a glide frame
//! against the number of rows *that exist* — which here is the number of rows in the data, because
//! there is no window between the two.
//!
//! It prints its slope beside the virtualised one for the same reason it exists: so that the ratio
//! between them is on the page whenever somebody reaches for this number.

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use zgui::app::Fonts;
use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui::vocab::{
    Modifiers, PointerAction, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase,
    Timestamp, WheelEvent,
};
use zgui_bench::reference::watch::{self, Watching};
use zgui_bench::reference::{fit, sample};
use zgui_platform_headless::Harness;

/// How wide the window opens, in CSS pixels.
const WIDTH: f32 = 1000.0;

/// How tall it opens, which is also the port.
const HEIGHT: f32 = 800.0;

/// How many rows the four documents hold.
///
/// Far short of the hundred thousand `list-slope` runs, and deliberately: the point of this probe is
/// the *slope*, and a slope through four sizes says what a hundred thousand would cost without
/// anybody waiting for a hundred thousand rows to be built, styled and laid out four times over.
/// The extrapolation is the finding.
const ROWS: [usize; 4] = [2_500, 5_000, 10_000, 20_000];

/// One tick of a 120 Hz refresh.
const TICK: Duration = Duration::from_micros(8_333);

/// How many ticks of glide one notch is carried for — the same as `list-slope`'s.
const GLIDE_TICKS: usize = 24;

/// The sheet: the same row markup `list-slope` uses, in a plain scroll container.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .bench-list { width: 100%; height: 800px; overflow: scroll; flex-direction: column }
     .bench-line {
        flex-direction: row;
        height: 24px;
        align-items: center;
        gap: 16px;
        padding: 0 12px;
        background-color: #14161a;
        border-bottom: 1px solid #232833;
     }
     .bench-cell { width: 200px }"
);

/// Opens a document of `rows` rows, every one of them an element.
fn opened(rows: usize) -> (Harness<Runtime>, watch::Log) {
    let damage: watch::Log = Rc::new(RefCell::new(Vec::new()));
    let for_renderer = Rc::clone(&damage);
    let render = move |_surface: &std::sync::Arc<dyn zgui::platform::Surface>,
                       target: RenderTarget|
          -> Result<Box<dyn Renderer>, AppError> {
        let mut inner = zgui_testkit_scene::CaptureRenderer::new();
        inner.configure(target);
        let mut watching = Watching::new(Box::new(inner), Rc::clone(&for_renderer));
        watching.configure(target);
        Ok(Box::new(watching))
    };
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("unvirtualised-probe")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(render))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            let mut list = zgui::elements::column().class("bench-list");
            for index in 0..rows {
                list = list.child(
                    zgui::elements::row()
                        .class("bench-line")
                        .child(
                            zgui::elements::text()
                                .class("bench-cell")
                                .child(format!("row {index}")),
                        )
                        .child(
                            zgui::elements::text()
                                .class("bench-cell")
                                .child(format!("{}", index * 7 % 977)),
                        ),
                );
            }
            Box::new(list.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(WIDTH),
        DevicePx(HEIGHT),
    )));
    harness.settle(512);
    for _ in 0..8 {
        harness.advance(Duration::from_micros(16_667));
        harness.pump();
    }
    harness.settle(512);
    damage.borrow_mut().clear();
    (harness, damage)
}

/// The middle of the port.
fn middle() -> Point<CssPx, Css> {
    Point::new(CssPx(WIDTH / 2.0), CssPx(HEIGHT / 2.0))
}

/// One wheel notch.
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

/// One notch and the glide it starts, the same pass `list-slope`'s wheel gesture drives.
fn pass(harness: &mut Harness<Runtime>, turn: usize) -> Duration {
    let lines = if (turn / 8).is_multiple_of(2) {
        6.0
    } else {
        -6.0
    };
    let started = Instant::now();
    harness.deliver_to_first(notch(lines));
    harness.settle(64);
    for _ in 0..GLIDE_TICKS {
        harness.advance(TICK);
        harness.pump();
    }
    started.elapsed()
}

/// How many rows the document laid out.
fn boxes(harness: &Harness<Runtime>) -> usize {
    harness.app().windows()[0].layout().borrow().keys().len()
}

fn main() {
    println!(
        "This is a diagnostic probe and not a reference workload: it characterises a slope nobody \
         is expected to hit, and nothing in `cargo xtask` runs it. See the module documentation."
    );
    let mut points = Vec::new();
    for rows in ROWS {
        let (mut harness, damage) = opened(rows);
        let laid_out = boxes(&harness);
        assert!(
            laid_out >= rows,
            "a {rows}-row document laid out {laid_out} boxes, which is fewer than one per row — so \
             this is not the document the probe is about",
        );
        harness.deliver_to_first(SurfaceEvent::Pointer {
            action: PointerAction::Moved,
            event: PointerEvent::mouse(middle()),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        harness.settle(64);
        let cost = sample::median_ns(|turn| pass(&mut harness, turn));
        damage.borrow_mut().clear();
        pass(&mut harness, 0);
        let frames = damage.borrow().len();
        assert!(frames > 0, "a glide over {rows} rows drew no frames");
        #[expect(
            clippy::cast_precision_loss,
            reason = "a box count is in the tens of thousands and a frame count in the tens"
        )]
        let (axis, per_frame) = (laid_out as f64, cost / frames as f64);
        println!(
            "PROBE rows={rows} boxes={laid_out} frames={frames} glide_ns_per_frame={per_frame:.0} \
             damage={:?}",
            watch::mean_fraction(&damage),
        );
        points.push((axis, per_frame));
    }
    match fit::slope(&points) {
        Some(slope) => println!(
            "PROBE slope {slope:.1} ns per box per drawn frame, over a document that mounts every \
             row.\nPROBE this number is keyed to this machine and gates nothing. Read it against \
             the virtualised list's own slope, which `cargo run --release -p zgui-bench --bin \
             list-slope` prints: the ratio between them is how much of any scroll improvement is \
             an improvement to a document nobody ships."
        ),
        None => {
            eprintln!("PROBE BROKEN: the four sizes did not determine a line.");
            std::process::exit(1);
        }
    }
}
