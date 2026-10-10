//! SVG static: a grid of multi-colour vector documents, left alone and then scrolled.
//!
//! Four distinct sources, each with solid colours, a linear and a radial gradient and a clip, so
//! every document mixes shapes the mask route can take with shapes it cannot.

use zgui::geom::{CssPx, Point};
use zgui::prelude::*;
use zgui::view;
use zgui::view::{Anchor, BuildCx, IntoView};

use crate::scenario::vector::{Stretch, opened, scroll};

/// How many ticks the still stretch runs.
const STILL_TICKS: usize = 60;

/// How many ticks the scroll runs.
const SCROLL_TICKS: usize = 300;

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

/// Eight rows of twelve documents.
fn grid() -> impl IntoView {
    view! {
        column(class = "svg-port") {
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
        }
    }
}

/// Runs one variant.
pub(super) fn run(variant: &str) {
    assert_eq!(variant, "grid", "unknown svg-static variant `{variant}`");
    let runtime = crate::scenario::fixture::custom(SHEET, |cx: &mut BuildCx<'_>| {
        Box::new(grid().into_view().build(cx)) as Box<dyn Anchor>
    });
    let mut harness = opened(runtime);
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
}
