//! A diagnostic probe: five hundred skeleton loaders animating their background colour.
//!
//! **This is a diagnostic probe, not a reference workload.** It gates nothing.
//!
//! Each box is a rounded placeholder whose `background-color` a keyframed animation moves between
//! two greys once a second, as a loading list does. The probe steps the clock one 60 Hz refresh at
//! a time, first eight steps to warm up and then thirty that it reports. Each reported step prints
//! its counters, its wall time and, with `ZGUI_BENCH_GPU=1 ZGUI_GPU_TIMING=1`, its GPU time.
//!
//! ```text
//! cargo build --release -p zgui-bench --bin skeleton
//! ZGUI_BENCH_GPU=1 ZGUI_GPU_TIMING=1 target/release/skeleton
//! ```

#![forbid(unsafe_code)]

use zgui::view;
use zgui::view::{Anchor, AnyView, BuildCx, IntoView, View};
use zgui_bench::probe::{self, Device};

/// How many boxes animate.
const BOXES: usize = 500;

/// Steps run before the reported ones, past the first paint.
const WARM: usize = 8;

/// Steps reported: half a second.
const STEPS: usize = 30;

/// The window's size.
const WIDTH: f32 = 1280.0;

/// The window's height.
const HEIGHT: f32 = 720.0;

/// The sheet: 25 boxes to a row, 20 rows.
const SHEET: &str = "
    :root { background-color: #14161a; font-family: \"Noto Sans\"; font-size: 13px }
    @keyframes probe-shimmer {
        from { background-color: #2a2f3a }
        50% { background-color: #3d4452 }
        to { background-color: #2a2f3a }
    }
    .grid { display: flex; flex-direction: row; flex-wrap: wrap; gap: 8px; width: 1200px; padding: 16px }
    .bone {
        width: 40px; height: 14px; border-radius: 4px; background-color: #2a2f3a;
        animation-name: probe-shimmer; animation-duration: 1000ms;
        animation-timing-function: ease-in-out; animation-iteration-count: infinite;
    }";

/// The grid of boxes.
fn grid() -> impl IntoView {
    let bones: Vec<AnyView> = (0..BOXES)
        .map(|_| AnyView::new(view! { box(class = "bone") }))
        .collect();
    view! { box(class = "grid") { {bones} } }
}

fn main() {
    let device = Device::from_env();
    println!("PROBE\tskeleton\tdevice={}\tboxes={BOXES}", device.name());
    let opened = probe::open(
        "skeleton",
        WIDTH,
        HEIGHT,
        SHEET,
        device,
        |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> { Box::new(grid().into_view().build(cx)) },
    );
    let mut opened = opened.unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    probe::census("skeleton", &opened);
    probe::step(&mut opened, WARM);
    let steps = probe::step(&mut opened, STEPS);
    let probe::Opened { mut harness, draws } = opened;
    harness.shut_down();
    // The renderer drops with the harness, which waits for the last GPU results.
    drop(harness);
    probe::report("skeleton", &steps, &draws.borrow());
}
