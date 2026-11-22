//! A fling the compositor carries over recorded rows draws every frame as a whole redraw would.
//!
//! The rows are solid and of distinct colours, so the picture of a port scrolled to any offset is
//! known: each device row of the port shows the row the offset puts there. Every frame of the
//! fling is checked against that picture at the offset its own top edge shows.

mod desktop;
mod device;
mod painted;

use zgui::geom::{DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::vocab::ScrollPhase;
use zgui_profile::Counter;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #101010 }
    .port { width: 300px; height: 200px; overflow: auto }
    .row { width: 300px; height: 20px; flex-shrink: 0 }";

/// How many rows the port holds, all within the margin the window records around it.
const ROWS: usize = 24;

/// The colour of row `index`.
fn colour(index: usize) -> (u8, u8, u8) {
    let index = u8::try_from(index).expect("few rows");
    (40 + index * 8, 200 - index * 6, 90 + index * 5)
}

#[test]
fn a_fling_over_recorded_rows_matches_a_whole_redraw_in_every_frame() {
    let Some(mut stage) = Stage::open(SHEET, || {
        view! {
            column(class = "port") {
                for index in move || (0..ROWS).collect::<Vec<_>>(), key = |index: &usize| *index {
                    box(class = "row", style:background-color = move || {
                        let (red, green, blue) = colour(index);
                        Some(format!("rgb({red}, {green}, {blue})"))
                    })
                }
            }
        }
    }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    stage.settle();
    stage.move_to(Point::new(DevicePx(100.0), DevicePx(100.0)));
    // One column of the port, clear of its scrollbar.
    let column = Rect::new(
        Point::new(DevicePx(100.0), DevicePx(0.0)),
        Size::new(DevicePx(1.0), DevicePx(200.0)),
    );

    stage.touchpad_in(0.0, ScrollPhase::Started);
    for _ in 0..4 {
        stage.touchpad_in(12.0, ScrollPhase::Moved);
    }
    let before = zgui_profile::counter::snapshot();
    stage.touchpad_in(0.0, ScrollPhase::Ended);
    let mut faults = Vec::new();
    let mut furthest = 0;
    for frame in 0..60 {
        let shown = stage.composed_colours_in(column);
        let Some(top) = (0..ROWS).find(|row| colour(*row) == shown[0]) else {
            faults.push(format!(
                "frame {frame}: the top of the port shows {:?}",
                shown[0]
            ));
            stage.tick();
            continue;
        };
        // How far into the top row the port starts, read off where the next row begins.
        let into = 20
            - shown
                .iter()
                .position(|seen| *seen != shown[0])
                .unwrap_or(20)
                .min(20);
        let offset = top * 20 + into;
        furthest = furthest.max(offset);
        let wrong = shown
            .iter()
            .enumerate()
            .filter(|(y, seen)| **seen != colour(((y + offset) / 20).min(ROWS - 1)))
            .count();
        if wrong > 0 {
            faults.push(format!("frame {frame} at {offset}: {wrong} pixels differ"));
        }
        stage.tick();
    }
    let moved = before.delta(&zgui_profile::counter::snapshot());

    assert!(faults.is_empty(), "{}", faults.join("\n"));
    assert!(
        furthest > 60,
        "the fling carried the rows {furthest} pixels"
    );
    assert!(
        moved.get(Counter::CompositorTicks) > moved.get(Counter::ZguiFrames),
        "the window ran {} frames against {} of the compositor's",
        moved.get(Counter::ZguiFrames),
        moved.get(Counter::CompositorTicks)
    );
}
