//! What one more box costs a resize, measured against what it costs the pipeline underneath.
//!
//! Resize is not what the compositor programme is about. Regressing it silently is not acceptable
//! either, and the shape it would regress into is a specific one: a configure that used to cost a
//! relayout and a repaint of the surface starts costing something proportional to the *document*
//! several times over. That shows up as a change in the **slope** — microseconds per box across
//! documents of different sizes — long before it shows up in any one number.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin resize-slope
//! ```
//!
//! # Why a ratio and not the slope itself
//!
//! Microseconds per box is a property of the machine as much as of the code. Recorded on one
//! machine and compared on another it fails for a reason nobody can act on, and the only way to
//! make it pass again is to record it again — which is a gate that gates nothing.
//!
//! So the slope is measured twice **in the same process, over the same four documents, minutes
//! apart at most**: once for a configure, and once for a change to the document's own content that
//! forces the same restyle, relayout and full repaint by a route that has nothing to do with the
//! window's extent. What is compared against a recorded value is the *ratio* of the two, which is
//! dimensionless: a machine twice as fast halves both slopes and leaves it exactly where it was,
//! and a change that makes a configure cost more than the work inside it moves it immediately.
//!
//! The slope in microseconds per box is printed beside the ratio and **gates nothing**. It is the
//! number a person reads when the ratio has moved and they want to know which half moved.

#![forbid(unsafe_code)]

use std::time::Duration;

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui_platform_headless::Harness;

mod fit;
mod measure;
mod verdict;

/// How wide the window opens, in CSS pixels.
const WIDTH: f32 = 1000.0;

/// How tall it opens.
const HEIGHT: f32 = 800.0;

/// The four document sizes, in rows.
///
/// Four rather than two because a slope taken from two points is a line through two points: it
/// cannot tell a cost proportional to the document from one proportional to its square, and the
/// second is exactly the regression this exists for. They quadruple so that the largest is
/// unmistakably larger than the noise on the smallest.
const ROWS: [usize; 4] = [64, 128, 256, 512];

/// How the document is laid out. Every row is a box with a box inside it.
const SHEET: &str = "root { display: block; width: 100%; height: 100%; overflow: scroll }
                     .row { display: block; width: 100%; height: 18px; padding: 2px }
                     .cell { display: block; width: 40%; height: 14px;
                             background-color: rgb(40, 44, 56) }
                     .warm .cell { background-color: rgb(90, 60, 40) }";

/// A renderer that records the display list and draws nowhere.
///
/// The measurement is of the pipeline that *produces* a frame — restyle, layout, paint, encode —
/// and a real graphics device would add a submission and a present whose cost is the driver's
/// rather than this workspace's, on both halves of the ratio and in different proportions.
fn capture(
    _surface: &std::sync::Arc<dyn zgui::platform::Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
    renderer.configure(target);
    Ok(Box::new(renderer))
}

/// Opens a document of `rows` rows, whose palette `warm` switches.
fn opened(rows: usize, warm: RwSignal<bool, zgui::reactive::LocalStorage>) -> Harness<Runtime> {
    let handler = App::new()
        .with_title("resize-slope")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(capture))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            let mut root = zgui::elements::column()
                .class("root")
                .class_toggle(zgui::view::ClassName::new("warm"), move || warm.get());
            for _ in 0..rows {
                root = root.child(
                    zgui::elements::column()
                        .class("row")
                        .child(zgui::elements::column().class("cell")),
                );
            }
            Box::new(root.into_view().build(cx))
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
    harness
}

/// How many boxes the document actually has, read off the layout rather than assumed.
fn boxes(harness: &Harness<Runtime>) -> usize {
    harness.app().windows()[0].layout().borrow().keys().len()
}

fn main() {
    let mut resizes = Vec::new();
    let mut contents = Vec::new();

    for rows in ROWS {
        let warm = RwSignal::new_local(false);
        let mut harness = opened(rows, warm);
        let boxes = boxes(&harness);
        let resize = measure::configures(&mut harness);
        let content = measure::content_changes(&mut harness, warm);
        println!("SIZE rows={rows} boxes={boxes} resize_us={resize:.3} content_us={content:.3}",);
        resizes.push((boxes as f64, resize));
        contents.push((boxes as f64, content));
    }

    let verdict = verdict::Verdict::of(&resizes, &contents);
    println!("{verdict}");
    if !verdict.passed() {
        std::process::exit(1);
    }
}
