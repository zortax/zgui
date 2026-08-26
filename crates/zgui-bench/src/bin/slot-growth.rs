//! A diagnostic probe: what a long session of streaming updates leaves behind.
//!
//! **This is not a CI reference workload.** It answers one report — CPU that climbs and
//! interaction that degrades over the first twenty minutes of a session — by driving the update
//! pattern a live agent thread produces: rows whose text changes over and over, some of them
//! ellipsized, in a window that stays open. What is watched is the scene's live table gauges and
//! the cost of the same update early against late.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin slot-growth
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

/// How many rows the fixture holds — a modest thread, so growth is the signal and not scale.
const ROWS: usize = 60;

/// How many updates the session streams.
const UPDATES: usize = 4_000;

/// The agent-log shapes that matter: an ellipsized one-line summary, and a wrapped body.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .log { display: flex; flex-direction: column; overflow-y: auto; height: 800px }
     .row { flex-direction: row; height: 20px; align-items: center; gap: 8px }
     .name { width: 90px }
     .summary { flex: 1 1 auto; white-space: nowrap; overflow: hidden;
                text-overflow: ellipsis }"
);

/// The streaming rows: a fixed name and a summary that keeps changing.
#[component]
fn Feed(
    /// One signal per row's summary.
    texts: Vec<zgui::reactive::RwSignal<String, zgui::reactive::LocalStorage>>,
) -> impl IntoView {
    view! {
        column(class = "log") {
            for entry in move || texts.clone().into_iter().enumerate(),
                key = |entry: &(usize, _)| entry.0
            {
                {feed_row(entry)}
            }
        }
    }
}

/// One streaming row.
fn feed_row(
    entry: (
        usize,
        zgui::reactive::RwSignal<String, zgui::reactive::LocalStorage>,
    ),
) -> impl IntoView {
    let (index, text) = entry;
    view! {
        row(class = "row") {
            label(class = "name") {{format!("tool-{index}")}}
            label(class = "summary") {{move || text.get()}}
        }
    }
}

fn main() {
    println!(
        "This is a diagnostic probe and not a reference workload: it drives a session of \
         streaming row updates and watches what the scene's tables keep."
    );

    let texts: Vec<zgui::reactive::RwSignal<String, zgui::reactive::LocalStorage>> = (0..ROWS)
        .map(|index| zgui::reactive::RwSignal::new_local(format!("starting tool {index}")))
        .collect();
    let mounted = texts.clone();

    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("slot-growth")
        .with_size(1000.0, 800.0)
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
            Box::new(
                view! { Feed(texts = mounted.clone()) }
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(1000.0),
        DevicePx(800.0),
    )));
    harness.settle(256);
    counter::reset();

    println!(
        "{:>7} {:>12} {:>12} {:>12} {:>10}",
        "update", "clips_live", "paints_live", "prepared", "cost_us"
    );
    let mut costs: Vec<f64> = Vec::new();
    for update in 0..UPDATES {
        // One tool row's summary moves, the way a running tool's does: longer, then replaced.
        let row = update % ROWS;
        let word = ["reading", "grepping", "building", "measuring", "writing"][update % 5];
        texts[row].set(format!(
            "{word} step {update} of the long streaming session, cut off somewhere past the edge"
        ));
        let started = Instant::now();
        harness.settle(16);
        harness.advance(Duration::from_millis(16));
        harness.pump();
        costs.push(started.elapsed().as_secs_f64() * 1e6);
        if update % 500 == 0 || update + 1 == UPDATES {
            costs.sort_by(f64::total_cmp);
            let p50 = costs[costs.len() / 2];
            println!(
                "{update:>7} {:>12} {:>12} {:>12} {p50:>10.1}",
                counter::get(Counter::ClipEntriesLive),
                counter::get(Counter::PaintEntriesLive),
                counter::get(Counter::SideTableSlotsPrepared),
                p50 = p50,
            );
            costs.clear();
        }
    }
    harness.shut_down();
}
