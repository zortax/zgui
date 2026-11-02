//! A diagnostic probe: one panel of about three hundred painted fragments fading its opacity.
//!
//! **This is a diagnostic probe, not a reference workload.** It gates nothing.
//!
//! The panel holds 150 swatches, each a rounded background, and 150 text labels. A keyframed
//! animation moves the panel's opacity between 1 and 0.2 once a second. The probe steps the clock
//! one 60 Hz refresh at a time, first eight steps to warm up and then thirty that it reports. Each
//! reported step prints its counters, its wall time and, with
//! `ZGUI_BENCH_GPU=1 ZGUI_GPU_TIMING=1`, its GPU time.
//!
//! Three variants:
//!
//! - `disjoint`: no child's ink overlaps another's, so the fade could be applied to each child.
//! - `overlap`: each child overlaps its neighbour, so the fade needs the panel isolated as a group.
//! - `backed`: as `disjoint`, with a background on the panel under every child.
//!
//! The first two panels paint nothing of their own. Read `presented` and `damage_px` on their
//! `FRAME` lines: a step that damaged nothing drew no fade.
//!
//! ```text
//! cargo build --release -p zgui-bench --bin fade-panel
//! target/release/fade-panel              # every variant, each in a process of its own
//! ZGUI_BENCH_GPU=1 ZGUI_GPU_TIMING=1 target/release/fade-panel overlap
//! ```

#![forbid(unsafe_code)]

use zgui::view;
use zgui::view::{Anchor, AnyView, BuildCx, IntoView, View};
use zgui_bench::probe::{self, Device};

/// How many swatches, and how many labels: three hundred painted children together.
const PAIRS: usize = 150;

/// Steps run before the reported ones, past the first paint.
const WARM: usize = 8;

/// Steps reported: half a second, half of one fade.
const STEPS: usize = 30;

/// The window's size.
const WIDTH: f32 = 1280.0;

/// The window's height.
const HEIGHT: f32 = 720.0;

/// The sheet. Only `.backed` gives the panel a background; otherwise only its children paint.
const SHEET: &str = "
    :root { background-color: #14161a; color: #e7ecf5; font-family: \"Noto Sans\"; font-size: 13px }
    .stage { display: block; padding: 24px }
    @keyframes probe-fade {
        from { opacity: 1 }
        50% { opacity: 0.2 }
        to { opacity: 1 }
    }
    .panel {
        display: flex; flex-direction: row; flex-wrap: wrap; gap: 6px; width: 1200px;
        animation-name: probe-fade; animation-duration: 1000ms;
        animation-timing-function: linear; animation-iteration-count: infinite;
    }
    .swatch { width: 48px; height: 20px; border-radius: 3px; background-color: #3b82f6 }
    .tag { width: 64px; height: 20px }
    .overlap > .swatch, .overlap > .tag { margin-left: -20px }
    .backed { background-color: #1b1e24 }";

/// The panel, with `class` on it.
fn panel(class: &'static str) -> impl IntoView {
    let children: Vec<AnyView> = (0..PAIRS)
        .flat_map(|index| {
            [
                AnyView::new(view! { box(class = "swatch") }),
                AnyView::new(view! { label(class = "tag") {{format!("item {index}")}} }),
            ]
        })
        .collect();
    view! {
        column(class = "stage") {
            box(class = class) { {children} }
        }
    }
}

fn main() {
    let variant = probe::variant_or_each(&["disjoint", "overlap", "backed"]);
    let class = match variant.as_str() {
        "disjoint" => "panel",
        "overlap" => "panel overlap",
        "backed" => "panel backed",
        other => {
            eprintln!("usage: fade-panel [disjoint|overlap|backed], not `{other}`");
            std::process::exit(2);
        }
    };
    let device = Device::from_env();
    let name = format!("fade-panel/{variant}");
    println!(
        "PROBE\t{name}\tdevice={}\tchildren={}",
        device.name(),
        2 * PAIRS
    );
    let opened = probe::open(
        "fade-panel",
        WIDTH,
        HEIGHT,
        SHEET,
        device,
        move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(panel(class).into_view().build(cx))
        },
    );
    let mut opened = opened.unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    probe::census(&name, &opened);
    probe::step(&mut opened, WARM);
    let steps = probe::step(&mut opened, STEPS);
    let probe::Opened { mut harness, draws } = opened;
    harness.shut_down();
    // The renderer drops with the harness, which waits for the last GPU results.
    drop(harness);
    probe::report(&name, &steps, &draws.borrow());
}
