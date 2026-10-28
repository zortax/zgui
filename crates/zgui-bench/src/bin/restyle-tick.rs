//! A diagnostic probe: three hundred rows whose animations go through the cascade on every frame.
//!
//! **This is a diagnostic probe, not a reference workload.** It gates nothing.
//!
//! Each row is a box with a text label, styled through custom properties as a component library
//! styles it. An animation on a property outside the paint and placement tiers makes every row
//! cascade again on every frame. The probe steps the clock one 60 Hz refresh at a time, first eight
//! steps to warm up and then sixty that it reports.
//!
//! Four variants:
//!
//! - `padding`: a keyframed `padding-left`, which moves the row's text.
//! - `width`: a keyframed `width`.
//! - `color`: a keyframed `color`, which every label inherits.
//! - `flip`: transitions of `color`, `box-shadow` and `padding-left`, started by a class on the list
//!   that flips every thirty steps, as a theme flip starts them.
//!
//! ```text
//! cargo build --release -p zgui-bench --bin restyle-tick
//! target/release/restyle-tick              # every variant, each in a process of its own
//! ZGUI_BENCH_GPU=1 target/release/restyle-tick flip
//! ```

#![forbid(unsafe_code)]

use zgui::prelude::{Get, Update};
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view::{Anchor, BuildCx, ClassName, IntoView, View};
use zgui_bench::probe::{self, Device};

/// How many rows animate.
const ROWS: usize = 300;

/// Steps run before the reported ones, past the first paint.
const WARM: usize = 8;

/// Steps reported: one second.
const STEPS: usize = 60;

/// Steps between two flips of the `flip` variant: half a second, the length of its transitions.
const FLIP_EVERY: usize = 30;

/// The window's size.
const WIDTH: f32 = 1280.0;

/// The window's height.
const HEIGHT: f32 = 720.0;

/// The sheet every variant shares: tokens on the root, rows that read them.
const SHEET: &str = "
    :root {
        --bg: #14161a; --fg: #e7ecf5; --muted: #8a93a6; --line: #2a2f3a; --accent: #3b82f6;
        --pad-x: 8px; --pad-y: 2px; --radius: 4px; --size: 13px; --gap: 4px;
        --shadow: 0 0 0 1px #2a2f3a; --shadow-on: 0 0 0 2px #3b82f6;
        --fg-on: #ffffff; --pad-on: 24px;
        background-color: var(--bg); color: var(--fg); font-family: \"Noto Sans\";
        font-size: var(--size);
    }
    .list {
        display: flex; flex-direction: row; flex-wrap: wrap; gap: var(--gap); width: 1240px;
        padding: var(--pad-x);
    }
    .row {
        display: block; width: 120px; height: 16px; padding: var(--pad-y) var(--pad-x);
        border: 1px solid var(--line); border-radius: var(--radius); color: var(--muted);
        background-color: var(--bg); box-shadow: var(--shadow);
    }
    @keyframes probe-pad { from { padding-left: 4px } 50% { padding-left: 24px } to { padding-left: 4px } }
    @keyframes probe-width { from { width: 100px } 50% { width: 140px } to { width: 100px } }
    @keyframes probe-color { from { color: #8a93a6 } 50% { color: #ffffff } to { color: #8a93a6 } }
    .padding > .row, .width > .row, .color > .row {
        animation-duration: 1000ms; animation-timing-function: linear;
        animation-iteration-count: infinite;
    }
    .padding > .row { animation-name: probe-pad }
    .width > .row { animation-name: probe-width }
    .color > .row { animation-name: probe-color }
    .flip > .row {
        transition-property: color, box-shadow, padding-left; transition-duration: 500ms;
        transition-timing-function: linear;
    }
    .flip.on > .row { color: var(--fg-on); box-shadow: var(--shadow-on); padding-left: var(--pad-on) }";

/// The list, with `variant` as a class on it and `on` toggling the `on` class.
fn list(variant: &'static str, on: RwSignal<bool, LocalStorage>) -> impl IntoView {
    let mut list = zgui::elements::column()
        .class(format!("list {variant}"))
        .class_toggle(ClassName::new("on"), move || on.get());
    for index in 0..ROWS {
        list = list.child(
            zgui::elements::column()
                .class("row")
                .child(zgui::elements::text().child(format!("row {index}"))),
        );
    }
    list
}

fn main() {
    let variant = probe::variant_or_each(&["padding", "width", "color", "flip"]);
    let class: &'static str = match variant.as_str() {
        "padding" => "padding",
        "width" => "width",
        "color" => "color",
        "flip" => "flip",
        other => {
            eprintln!("usage: restyle-tick [padding|width|color|flip], not `{other}`");
            std::process::exit(2);
        }
    };
    let device = Device::from_env();
    let name = format!("restyle-tick/{variant}");
    println!("PROBE\t{name}\tdevice={}\trows={ROWS}", device.name());
    let on = RwSignal::new_local(false);
    let opened = probe::open(
        "restyle-tick",
        WIDTH,
        HEIGHT,
        SHEET,
        device,
        move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(list(class, on).into_view().build(cx))
        },
    );
    let mut opened = opened.unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    probe::census(&name, &opened);
    probe::step(&mut opened, WARM);
    let mut steps = Vec::with_capacity(STEPS);
    while steps.len() < STEPS {
        if class == "flip" && steps.len() % FLIP_EVERY == 0 {
            on.update(|on| *on = !*on);
        }
        steps.extend(probe::step(&mut opened, 1));
    }
    let restyled: u64 = steps
        .iter()
        .map(|step| step.counters.elements_restyled + step.counters.elements_recascaded)
        .sum();
    println!(
        "RESTYLE\t{name}\telements_styled={restyled}\tsteps={}",
        steps.len()
    );
    let probe::Opened { mut harness, draws } = opened;
    harness.shut_down();
    // The renderer drops with the harness, which waits for the last GPU results.
    drop(harness);
    probe::report(&name, &steps, &draws.borrow());
}
