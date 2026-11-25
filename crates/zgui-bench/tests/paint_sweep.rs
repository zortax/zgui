//! What boxes with animated colours leave in the paint table when they come and go.
//!
//! A box whose background animates draws it through a paint slot named after the box. The box
//! that is unmounted leaves its slot behind with no holds, and a list that mounts new animated
//! boxes for ever would grow the table for ever. The runtime sweeps the table on a stride of
//! frames, so the table stays at what the last few hundred frames drew. A box that stays mounted
//! keeps the paints it draws through every sweep.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

#![forbid(unsafe_code)]

use std::time::Duration;

use iceblink::RenderTarget;
use zgui::app::Fonts;
use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx, IntoView, View};
use zgui::{component, view};
use zgui_platform_headless::Harness;
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_testkit_compositor::Drawing;

/// How many animated boxes one generation mounts.
const BOXES: usize = 40;

/// How many frames each generation is drawn for before the next replaces it.
const FRAMES_PER_GENERATION: usize = 3;

/// How many generations the churn runs.
const GENERATIONS: usize = 400;

/// One tick of a 60 Hz refresh.
const TICK: Duration = Duration::from_micros(16_667);

/// Boxes whose background pulses for as long as they are mounted, and one box that stays and
/// animates its background and its outline.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a }
     .keep { width: 30px; height: 30px; margin: 8px; background-color: transparent;
             outline: 2px solid transparent; animation: glow 600ms linear infinite }
     @keyframes glow {
         from { background-color: #98c379; outline-color: #c678dd }
         to { background-color: #e5c07b; outline-color: #56b6c2 }
     }
     .grid { display: flex; flex-direction: row; flex-wrap: wrap; gap: 4px }
     .cell { width: 20px; height: 20px; background-color: transparent;
             animation: pulse 600ms linear infinite }
     @keyframes pulse {
         from { background-color: #61afef }
         to { background-color: #e06c75 }
     }"
);

/// One generation of animated boxes, keyed by generation so each one mounts new boxes.
#[component]
fn Grid(
    /// Which generation is mounted.
    generation: zgui::reactive::RwSignal<usize, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    view! {
        row(class = "grid") {
            for cell in move || {
                let base = generation.get() * BOXES;
                base..base + BOXES
            }, key = |cell: &usize| *cell {
                {cell_of(cell)}
            }
        }
    }
}

/// One animated box.
fn cell_of(_cell: usize) -> impl IntoView {
    view! { box(class = "cell") {} }
}

/// Opens the window over the machine's own fonts.
fn opened(
    generation: zgui::reactive::RwSignal<usize, zgui::reactive::LocalStorage>,
) -> Harness<Runtime> {
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let handler = App::new()
        .with_title("paint-sweep")
        .with_size(800.0, 600.0)
        .with_stylesheet(SHEET)
        .with_compositor(zgui_testkit_compositor::driver(zgui::app::compositor(
            &fonts,
        )))
        .with_renderer(Box::new(
            |_surface: &std::sync::Arc<dyn zgui::platform::Surface>,
             target: RenderTarget|
             -> Result<iceblink::RendererFactory, AppError> {
                Ok(zgui_testkit_compositor::screen(target))
            },
        ))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(
                view! {
                    column(class = "root") {
                        box(class = "keep") {}
                        Grid(generation = generation)
                    }
                }
                .into_view()
                .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(800.0),
        DevicePx(600.0),
    )));
    harness.settle(64);
    harness
}

#[test]
fn animated_boxes_that_come_and_go_keep_the_paint_table_flat() {
    let generation = zgui::reactive::RwSignal::new_local(0_usize);
    let mut harness = opened(generation);
    if !COUNTERS_ENABLED {
        return;
    }

    // The sweep runs on a stride, so the count rises and falls; the peaks of two stretches of
    // the churn are what is compared.
    let mut earlier = 0;
    let mut later = 0;
    for next in 1..=GENERATIONS {
        generation.set(next);
        for _ in 0..FRAMES_PER_GENERATION {
            harness.advance(TICK);
            harness.pump();
        }
        let live = counter::get(Counter::PaintEntriesLive);
        if next > GENERATIONS / 4 && next <= GENERATIONS / 2 {
            earlier = earlier.max(live);
        } else if next > GENERATIONS * 3 / 4 {
            later = later.max(live);
        }
    }
    assert!(
        earlier >= 1_024,
        "the churn never filled the table to the size where sweeps run: {earlier} entries"
    );
    let drawing = drawing();
    let quads = drawing.quads();
    assert!(!quads.is_empty(), "the last frame drew no box");
    for quad in quads {
        for role in ["fill", "stroke"] {
            let paint = quad.paint(role).unwrap_or_default();
            assert!(
                !paint.contains("<missing>"),
                "a sweep freed a paint the last frame drew: {}",
                quad.text()
            );
            assert!(
                !paint.contains("<mismatched>"),
                "a quad draws a paint of another family than it names: {}",
                quad.text()
            );
        }
    }
    let mounted = (GENERATIONS * BOXES) as u64;
    assert!(
        later < mounted / 4,
        "{later} live paint entries after {mounted} animated boxes came and went"
    );
    assert!(
        later <= earlier + earlier / 10,
        "the paint table grew from a peak of {earlier} to one of {later} entries as the churn went on"
    );
}

/// Returns what the window opened last on this thread shows.
fn drawing() -> Drawing {
    zgui_testkit_compositor::screens()
        .last()
        .and_then(zgui_testkit_compositor::Shown::drawing)
        .expect("the window drew a frame")
}
