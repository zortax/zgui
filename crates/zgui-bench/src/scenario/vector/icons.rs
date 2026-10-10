//! Icons scroll: a port of icons carried past by the wheel.
//!
//! `icons` is sixty rows of twelve library icons, every one a solid `currentColor` outline, so
//! every shape can take the mask route and nothing needs the general rasteriser. `gallery` is the
//! shipped gallery at `s13`, whose drawings include gradients and clips: it is the reference for
//! a document that mixes both routes.

use zgui::geom::{CssPx, Point};
use zgui::prelude::*;
use zgui::view;
use zgui::view::{Anchor, BuildCx, IntoView};
use zgui_ui_icons::IconData;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::{arrow, chevron, mark, status, ui};

use crate::scenario::vector::{Stretch, opened, scroll};

/// How many ticks the scroll runs.
const TICKS: usize = 300;

/// How many rows the port holds.
const ROWS: usize = 60;

/// The twelve icons a row draws, in order.
const ICONS: [IconData; 12] = [
    mark::CHECK,
    mark::PLUS,
    mark::CROSS,
    ui::SEARCH,
    ui::ELLIPSIS,
    chevron::CHEVRON_DOWN,
    chevron::CHEVRON_RIGHT,
    status::INFO,
    status::ALERT_TRIANGLE,
    status::CHECK_CIRCLE,
    arrow::ARROW_UP,
    arrow::ARROW_LEFT,
];

/// The port and its rows.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 12px }
     .icon-port { width: 800px; height: 600px; overflow: auto; flex-direction: column }
     .icon-row { flex-direction: row; gap: 16px; padding: 4px 8px; flex: none }"
);

/// Sixty rows of the twelve icons.
fn grid() -> impl IntoView {
    view! {
        column(class = "icon-port") {
            for _line in move || 0..ROWS, key = |line: &usize| *line {
                row(class = "icon-row") {
                    for index in move || 0..ICONS.len(), key = |index: &usize| *index {
                        Icon(icon = ICONS[index], size = IconSize::Xl)
                    }
                }
            }
        }
    }
}

/// Runs one variant.
pub(super) fn run(variant: &str) {
    let (runtime, at) = match variant {
        "icons" => (
            crate::scenario::fixture::custom(SHEET, |cx: &mut BuildCx<'_>| {
                Box::new(grid().into_view().build(cx)) as Box<dyn Anchor>
            }),
            Point::new(CssPx(400.0), CssPx(300.0)),
        ),
        "gallery" => (
            crate::gallery::runtime("s13"),
            Point::new(
                CssPx(crate::gallery::WIDTH / 2.0),
                CssPx(crate::gallery::HEIGHT / 2.0),
            ),
        ),
        other => panic!("unknown icons-scroll variant `{other}`"),
    };
    // Counters only: opening runs its frames before any tick is measured.
    let open = Stretch::begin("icons-scroll", variant, "open");
    let mut harness = opened(runtime);
    open.end();
    scroll(&mut harness, ("icons-scroll", variant, "scroll"), at, TICKS);
}
