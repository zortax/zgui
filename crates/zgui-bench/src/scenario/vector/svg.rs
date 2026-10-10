//! SVG static: a grid of multi-colour vector documents, opened, left alone, scrolled, pinched and
//! left to settle.
//!
//! Four distinct sources, each with solid colours, a linear and a radial gradient and a clip, so
//! every document mixes shapes the mask route can take with shapes it cannot. The pinch scales the
//! port through an inline `transform`.
//!
//! `huge` is one 6000 by 4000 illustration of polygons, ramps, clips and strokes in a 1200 by 480
//! port: too large for one layer, so it is drawn in tiles. It is shown after the window opens, so
//! the `first` stretch is the frame that shows it. Then it is left alone, scrolled, and panned
//! sideways through an inline `transform`.

use zgui::geom::{CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view;
use zgui::view::{Anchor, BuildCx, IntoView};

use crate::scenario::vector::{Lcg, Stretch, opened, scroll, settle};

/// How many ticks the still stretch runs.
const STILL_TICKS: usize = 60;

/// How many ticks the scroll runs.
const SCROLL_TICKS: usize = 300;

/// How many ticks the pinch and the settle each run.
const PINCH_TICKS: usize = 30;

/// How many rows and columns the grid holds.
const ROWS: usize = 8;

/// How many documents one row holds.
const COLUMNS: usize = 12;

/// The four documents.
const SOURCES: [&str; 4] = [
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96">
  <defs>
    <linearGradient id="a" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#4f8cff"/><stop offset="1" stop-color="#1b2a5c"/></linearGradient>
    <radialGradient id="b" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#ffd166"/><stop offset="1" stop-color="#ef476f"/></radialGradient>
    <clipPath id="c"><circle cx="48" cy="48" r="30"/></clipPath>
  </defs>
  <rect x="4" y="4" width="88" height="88" rx="12" fill="url(#a)"/>
  <circle cx="48" cy="48" r="26" fill="url(#b)"/>
  <g clip-path="url(#c)"><rect x="20" y="40" width="56" height="16" fill="#06d6a0"/></g>
  <path d="M14 80 L30 64 L46 80 Z" fill="#ffffff"/>
  <path d="M60 18 L82 18 L82 26 L60 26 Z" fill="#118ab2"/>
</svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96">
  <defs>
    <linearGradient id="a" x1="0" y1="1" x2="0" y2="0"><stop offset="0" stop-color="#2a9d8f"/><stop offset="1" stop-color="#e9c46a"/></linearGradient>
    <radialGradient id="b" cx="0.3" cy="0.3" r="0.7"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#264653"/></radialGradient>
    <clipPath id="c"><rect x="16" y="16" width="64" height="40"/></clipPath>
  </defs>
  <circle cx="48" cy="48" r="44" fill="url(#b)"/>
  <path d="M8 88 L48 20 L88 88 Z" fill="url(#a)"/>
  <g clip-path="url(#c)"><circle cx="48" cy="56" r="28" fill="#e76f51"/></g>
  <rect x="40" y="70" width="16" height="16" fill="#f4a261"/>
  <path d="M10 10 L22 10 L22 22 Z" fill="#8ecae6"/>
</svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96">
  <defs>
    <linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#8338ec"/><stop offset="1" stop-color="#3a86ff"/></linearGradient>
    <radialGradient id="b" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#fb5607"/><stop offset="1" stop-color="#ffbe0b"/></radialGradient>
    <clipPath id="c"><path d="M48 8 L88 48 L48 88 L8 48 Z"/></clipPath>
  </defs>
  <rect x="0" y="0" width="96" height="96" fill="#1d1d2c"/>
  <g clip-path="url(#c)"><rect x="0" y="0" width="96" height="96" fill="url(#a)"/></g>
  <circle cx="48" cy="48" r="18" fill="url(#b)"/>
  <path d="M20 20 L32 20 L32 32 L20 32 Z" fill="#ff006e"/>
  <path d="M64 64 L76 64 L76 76 L64 76 Z" fill="#ffffff"/>
</svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96">
  <defs>
    <linearGradient id="a" x1="1" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ef233c"/><stop offset="1" stop-color="#2b2d42"/></linearGradient>
    <radialGradient id="b" cx="0.5" cy="0.6" r="0.5"><stop offset="0" stop-color="#edf2f4"/><stop offset="1" stop-color="#8d99ae"/></radialGradient>
    <clipPath id="c"><circle cx="48" cy="40" r="24"/></clipPath>
  </defs>
  <rect x="8" y="8" width="80" height="80" rx="40" fill="url(#b)"/>
  <g clip-path="url(#c)"><path d="M0 40 L96 40 L96 96 L0 96 Z" fill="url(#a)"/></g>
  <path d="M30 70 L66 70 L66 78 L30 78 Z" fill="#d90429"/>
  <circle cx="20" cy="20" r="6" fill="#2b2d42"/>
  <circle cx="76" cy="20" r="6" fill="#06d6a0"/>
</svg>"##,
];

/// The port and its cells.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 12px }
     .svg-port { width: 1200px; height: 480px; overflow: auto; flex-direction: column; gap: 4px }
     .svg-row { flex-direction: row; gap: 4px; flex: none }
     .svg-cell { width: 96px; height: 96px; flex: none }"
);

/// Eight rows of twelve documents, the port scaled by `zoom`.
fn grid(zoom: RwSignal<f64, LocalStorage>) -> impl IntoView {
    let rows = view! {
        for line in move || 0..ROWS, key = |line: &usize| *line {
            row(class = "svg-row") {
                for index in move || 0..COLUMNS, key = |index: &usize| *index {
                    {zgui::elements::vector()
                        .class("svg-cell")
                        .document(SOURCES[(line + index) % SOURCES.len()])
                        .into_view()}
                }
            }
        }
    };
    zgui::elements::column()
        .class("svg-port")
        .style_property("transform", move || {
            let zoom = zoom.get();
            (zoom != 1.0).then(|| format!("scale({zoom})"))
        })
        .child(rows)
}

/// The huge illustration's size, in CSS pixels.
const HUGE: (f64, f64) = (6000.0, 4000.0);

/// How many polygons the huge illustration holds.
const HUGE_POLYGONS: usize = 480;

/// How many stroked lines it holds.
const HUGE_LINES: usize = 160;

/// How many ticks the pan runs, and how far each moves.
const PAN_TICKS: usize = 120;

/// How far one pan tick moves, in CSS pixels.
const PAN_STEP: f64 = 3.0;

/// The eight colours of the huge illustration.
const PALETTE: [&str; 8] = [
    "#4f8cff", "#ef476f", "#06d6a0", "#ffd166", "#8338ec", "#118ab2", "#f4a261", "#8d99ae",
];

/// The huge illustration: a ground, polygons in solid colours, ramps and clips, and strokes.
fn huge_source() -> &'static str {
    let mut random = Lcg::new(0x00C0_FFEE);
    let (width, height) = HUGE;
    let mut defs = String::new();
    let mut body =
        format!(r##"<rect x="0" y="0" width="{width}" height="{height}" fill="#1b1e26"/>"##);
    for index in 0..HUGE_POLYGONS {
        let cx = random.next() * width;
        let cy = random.next() * height;
        let radius = 40.0 + random.next() * 260.0;
        let corners = 3 + (random.next() * 5.0) as usize;
        let mut path = String::new();
        for corner in 0..corners {
            let angle = std::f64::consts::TAU * corner as f64 / corners as f64 + random.next();
            let reach = radius * (0.6 + 0.4 * random.next());
            let (x, y) = (cx + reach * angle.cos(), cy + reach * angle.sin());
            path.push_str(&format!(
                "{}{x:.1} {y:.1} ",
                if corner == 0 { "M" } else { "L" }
            ));
        }
        path.push('Z');
        let first = PALETTE[index % PALETTE.len()];
        let second = PALETTE[(index * 3 + 1) % PALETTE.len()];
        let fill = match index % 6 {
            0 | 1 => {
                defs.push_str(&format!(
                    r##"<linearGradient id="l{index}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{first}"/><stop offset="1" stop-color="{second}"/></linearGradient>"##
                ));
                format!("url(#l{index})")
            }
            2 => {
                defs.push_str(&format!(
                    r##"<radialGradient id="r{index}" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="{first}"/><stop offset="1" stop-color="{second}"/></radialGradient>"##
                ));
                format!("url(#r{index})")
            }
            _ => first.to_owned(),
        };
        if index % 8 == 5 {
            defs.push_str(&format!(
                r##"<clipPath id="c{index}"><circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}"/></clipPath>"##,
                radius * 0.7
            ));
            body.push_str(&format!(
                r##"<g clip-path="url(#c{index})"><path d="{path}" fill="{fill}"/></g>"##
            ));
        } else {
            body.push_str(&format!(r##"<path d="{path}" fill="{fill}"/>"##));
        }
    }
    for index in 0..HUGE_LINES {
        let mut x = random.next() * width;
        let mut y = random.next() * height;
        let mut path = format!("M{x:.1} {y:.1}");
        for _ in 0..12 {
            x += (random.next() - 0.5) * 300.0;
            y += (random.next() - 0.5) * 300.0;
            path.push_str(&format!(" L{x:.1} {y:.1}"));
        }
        let colour = PALETTE[(index + 3) % PALETTE.len()];
        body.push_str(&format!(
            r##"<path d="{path}" fill="none" stroke="{colour}" stroke-width="6"/>"##
        ));
    }
    Box::leak(
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}"><defs>{defs}</defs>{body}</svg>"##
        )
        .into_boxed_str(),
    )
}

/// The port and the huge illustration in it.
const HUGE_SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 12px }
     .huge-port { width: 1200px; height: 480px; overflow: auto; flex-direction: column }
     .huge { width: 6000px; height: 4000px; flex: none }"
);

/// The huge illustration in its port, hidden until `shown`, moved left by `pan` CSS pixels.
fn huge(shown: RwSignal<bool, LocalStorage>, pan: RwSignal<f64, LocalStorage>) -> impl IntoView {
    let source = huge_source();
    let drawing = zgui::elements::vector()
        .class("huge")
        .document(source)
        .style_property("display", move || (!shown.get()).then(|| "none".to_owned()))
        .style_property("transform", move || {
            let pan = pan.get();
            (pan != 0.0).then(|| format!("translateX({}px)", -pan))
        });
    zgui::elements::column()
        .class("huge-port")
        .child(drawing.into_view())
}

/// Runs the huge variant.
fn run_huge() {
    let shown = RwSignal::new_local(false);
    let pan = RwSignal::new_local(0.0_f64);
    let runtime = crate::scenario::fixture::custom(HUGE_SHEET, move |cx: &mut BuildCx<'_>| {
        Box::new(huge(shown, pan).into_view().build(cx)) as Box<dyn Anchor>
    });
    let mut harness = opened(runtime);
    // The frame that shows the drawing, then the frames it owes until every visible tile stands.
    let mut first = Stretch::begin("svg-static", "huge", "first");
    shown.set(true);
    first.tick(&mut harness);
    first.end();
    let mut open = Stretch::begin("svg-static", "huge", "open");
    for _ in 0..4 {
        open.tick(&mut harness);
    }
    open.end();
    settle(&mut harness);
    let mut still = Stretch::begin("svg-static", "huge", "static");
    for _ in 0..STILL_TICKS {
        still.tick(&mut harness);
    }
    still.end();
    scroll(
        &mut harness,
        ("svg-static", "huge", "scroll"),
        Point::new(CssPx(600.0), CssPx(240.0)),
        SCROLL_TICKS,
    );
    let mut panned = Stretch::begin("svg-static", "huge", "pan");
    for tick in 1..=PAN_TICKS {
        pan.set(PAN_STEP * tick as f64);
        panned.tick(&mut harness);
    }
    panned.end();
    settle(&mut harness);
}

/// Runs one variant.
pub(super) fn run(variant: &str) {
    if variant == "huge" {
        run_huge();
        return;
    }
    assert_eq!(variant, "grid", "unknown svg-static variant `{variant}`");
    let zoom = RwSignal::new_local(1.0_f64);
    let runtime = crate::scenario::fixture::custom(SHEET, move |cx: &mut BuildCx<'_>| {
        Box::new(grid(zoom).into_view().build(cx)) as Box<dyn Anchor>
    });
    // Counters only: opening runs its frames before any tick is measured.
    let open = Stretch::begin("svg-static", variant, "open");
    let mut harness = opened(runtime);
    open.end();
    let mut still = Stretch::begin("svg-static", variant, "static");
    for _ in 0..STILL_TICKS {
        still.tick(&mut harness);
    }
    still.end();
    scroll(
        &mut harness,
        ("svg-static", variant, "scroll"),
        Point::new(CssPx(600.0), CssPx(240.0)),
        SCROLL_TICKS,
    );

    let mut pinch = Stretch::begin("svg-static", variant, "pinch");
    for tick in 1..=PINCH_TICKS {
        zoom.set(1.0 + 0.6 * tick as f64 / PINCH_TICKS as f64);
        pinch.tick(&mut harness);
    }
    pinch.end();

    let mut still = Stretch::begin("svg-static", variant, "settle");
    for _ in 0..PINCH_TICKS {
        still.tick(&mut harness);
    }
    still.end();
    settle(&mut harness);
}
