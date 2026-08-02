//! Whether a scroll puts on the screen what a repaint of the same state would.
//!
//! A window redraws the rectangles it damaged and keeps everything else from the frame before, so
//! the only thing that settles whether a partial frame was right is the same document, in the same
//! state, drawn whole. That is what every fixture here compares: the pixels standing after a scroll
//! against the pixels of a full repaint taken immediately afterwards, with nothing in between that
//! could move a box.
//!
//! # Why a scroll is where this fails
//!
//! Content moves past a clip. A primitive whose ink misses the clip in force is refused rather than
//! drawn, so a row outside the port paints nothing at all while it is out there — and what the
//! paint stage records for it is that: nothing. The record travels with the row, and the row is
//! exactly the thing that moves. Once it reaches the port the record still matches on everything a
//! cache compares — same style, same clip, same transform, same size — so the row is redrawn by
//! replaying an empty range, at every position it passes through, for as long as the record
//! survives.
//!
//! The result is a band of the surface that the damage cleared and nobody painted, and it stays
//! that way: the frames afterwards no longer damage it. On a desktop it looks like a panel that
//! never arrives, and it is repaired only by something that forces the whole window to be drawn
//! again — which is why the comparison here is taken *before* the repaint it is compared to.
//!
//! # Why each fixture checks that it moved something
//!
//! A wheel turned the wrong way at the end of the content scrolls nothing, and a fixture that
//! compares two identical pictures passes whatever the engine does. So every round asserts that the
//! port is showing something other than what it showed before, and only then asks whether what it
//! is showing is right.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Rect};
use zgui::view;
use zgui::view::{AnyView, NodeRef};
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;
use crate::painted::words::ink;

/// A port shorter than what is inside it, and rows plain enough to be read off a photograph.
///
/// The rows are filled and labelled rather than drawn, because a drawing is already refused the
/// replay path and would exercise the guard that exists rather than the one this is about.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 16px; align-items: flex-start }
                     .port { width: 300px; height: 240px; overflow-y: scroll }
                     .tall { flex-direction: column; gap: 8px; align-items: flex-start }
                     .card { width: 200px; height: 48px; padding: 8px;
                             background-color: #2f6bff; color: #ffffff }";

/// One detent towards the end of the content, in lines.
const DOWN: f32 = 1.0;

/// One detent back towards the start.
const UP: f32 = -1.0;

/// How much of the port has to differ from its commonest colour before it counts as painted.
///
/// The rows fill most of it, so this is far below what a working port produces and far above what
/// an empty one does.
const MARKED: f32 = 0.2;

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// A scrollport with forty filled rows in it, most of them below the port to begin with.
fn rows(port: NodeRef) -> impl Fn() -> AnyView + use<> {
    move || {
        let cards =
            (0..40).map(|index| view! { row(class = "card") {text {{format!("row {index}")}}} });
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    column(class = "port", node_ref = port) {
                        column(class = "tall") {{cards.collect::<Vec<_>>()}}
                    }
                }
            }
        })
    }
}

/// Fails naming how many pixels of `rect` a full repaint disagrees with the standing frame about.
///
/// The repaint is taken here rather than by the caller so that nothing can run between the two
/// readings: a fixture that settled, or moved the clock, or asked the document anything that
/// scheduled a frame would be comparing two different states and calling the difference a fault.
fn assert_matches_a_repaint(stage: &mut Stage, rect: Rect<DevicePx, Device>, when: &str) {
    let live = stage.colours_in(rect);
    stage.repaint();
    let whole = stage.colours_in(rect);
    assert_eq!(
        live.len(),
        whole.len(),
        "{when}: the two readings cover different areas"
    );
    let differing = live
        .iter()
        .zip(whole.iter())
        .filter(|(one, two)| one != two)
        .count();
    assert_eq!(
        differing, 0,
        "{when}: pixels in {rect:?} were left showing something a repaint of the same state does \
         not draw"
    );
}

/// A row that was below the port when the window opened is on the screen once it is scrolled to.
#[test]
fn a_row_that_scrolled_into_the_port_is_on_the_screen_without_a_repaint() {
    let port = NodeRef::new();
    let mut stage = staged!(rows(port));
    let node = port.get().expect("the port was built");
    let rect = stage.rect_of(node);
    stage.repaint();
    assert!(
        ink(&stage, rect) > MARKED,
        "the port drew nothing before anything was scrolled"
    );

    stage.move_to(stage.centre_of(node));
    let mut seen = stage.colours_in(rect);
    for round in 1..=8 {
        stage.wheel(DOWN);
        stage.settle();
        let now = stage.colours_in(rect);
        assert!(
            now != seen,
            "detent {round} left the port showing exactly what it showed before, so the fixture is \
             asserting nothing about a scroll"
        );
        seen = now;
        assert_matches_a_repaint(&mut stage, rect, &format!("after detent {round}"));
        assert!(
            ink(&stage, rect) > MARKED,
            "after detent {round}: the port is flat, so the rows scrolled to were never painted"
        );
    }
}

/// Scrolling back up puts the rows that left the port at the top back on the screen.
///
/// The upward half is not the downward half repeated. A row that leaves through the *top* of the
/// port has its painting cut short there rather than never made, so what its record holds is the
/// part of it that was still inside — and the part that was cut is the part that comes back first.
#[test]
fn rows_that_left_through_the_top_of_the_port_come_back_whole() {
    let port = NodeRef::new();
    let mut stage = staged!(rows(port));
    let node = port.get().expect("the port was built");
    let rect = stage.rect_of(node);
    stage.move_to(stage.centre_of(node));
    for _ in 0..8 {
        stage.wheel(DOWN);
    }
    stage.settle();
    stage.repaint();
    let mut seen = stage.colours_in(rect);

    for round in 1..=8 {
        stage.wheel(UP);
        stage.settle();
        let now = stage.colours_in(rect);
        assert!(
            now != seen,
            "detent {round} back up moved nothing, so the fixture is asserting nothing"
        );
        seen = now;
        assert_matches_a_repaint(&mut stage, rect, &format!("after detent {round} back up"));
        assert!(
            ink(&stage, rect) > MARKED,
            "after detent {round} back up: the port is flat"
        );
    }
}
