//! A diagnostic probe: what an animation that never ends leaves in the scene's tables.
//!
//! **This is not a CI reference workload.** It answers one report — CPU that climbs over minutes
//! in a window whose only motion is a spinner and a breathing glyph — by running exactly that: a
//! block of static rows, one element rotating for ever, one element breathing its opacity, and
//! nothing else. What is watched is each side table's live count and the cost of a tick, early
//! against late: a table that grows with the *duration* of an animation rather than its content
//! is the leak the report describes.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin anim-growth
//! ```

#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

use zgui::app::Fonts;
use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError};
use zgui::view::{Anchor, BuildCx, IntoView, View};
use zgui::{component, view};
use zgui_platform_headless::Harness;
use zgui_profile::{Counter, counter};

/// How many ticks the session runs — half a minute of a 60 Hz output.
const TICKS: usize = 2_000;

/// The report's shape: a spinner and a breathing glyph over static content.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .stage { display: flex; flex-direction: column; height: 600px }
     .row { height: 20px }
     @keyframes probe-turn {
         from { transform: rotate(0deg) }
         to { transform: rotate(360deg) }
     }
     @keyframes probe-breathe {
         from { opacity: 1 }
         50% { opacity: 0.35 }
         to { opacity: 1 }
     }
     .spin { width: 14px; height: 14px; background-color: #61afef;
             transform: rotate(0deg);
             animation-name: probe-turn; animation-duration: 900ms;
             animation-timing-function: linear; animation-iteration-count: infinite }
     .breathe { animation-name: probe-breathe; animation-duration: 1400ms;
                animation-timing-function: ease-in-out; animation-iteration-count: infinite }"
);

/// The stage: two animated elements and twenty static rows.
#[component]
fn Stage() -> impl IntoView {
    view! {
        column(class = "stage") {
            box(class = "spin") {}
            label(class = "breathe") {{"working"}}
            for index in move || 0..20_usize, key = |index: &usize| *index {
                {row_of(index)}
            }
        }
    }
}

/// One static row.
fn row_of(index: usize) -> impl IntoView {
    view! { label(class = "row") {{format!("static row {index}")}} }
}

fn main() {
    println!(
        "This is a diagnostic probe and not a reference workload: it runs a spinner and a \
         breathing glyph over static rows and watches what the scene's tables keep."
    );

    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("anim-growth")
        .with_size(800.0, 600.0)
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
            Box::new(view! { Stage() }.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(800.0),
        DevicePx(600.0),
    )));
    harness.settle(256);
    counter::reset();

    println!(
        "{:>6} {:>11} {:>12} {:>12} {:>10}",
        "tick", "clips_live", "paints_live", "spaces_live", "cost_us"
    );
    let mut costs: Vec<f64> = Vec::new();
    for tick in 0..TICKS {
        let started = Instant::now();
        harness.advance(Duration::from_millis(16));
        harness.pump();
        harness.settle(8);
        costs.push(started.elapsed().as_secs_f64() * 1e6);
        if tick % 250 == 0 || tick + 1 == TICKS {
            costs.sort_by(f64::total_cmp);
            let p50 = costs[costs.len() / 2];
            println!(
                "{tick:>6} {:>11} {:>12} {:>12} {p50:>10.1}",
                counter::get(Counter::ClipEntriesLive),
                counter::get(Counter::PaintEntriesLive),
                counter::get(Counter::SpatialNodesLive),
                p50 = p50,
            );
            costs.clear();
        }
    }
    harness.shut_down();
}
