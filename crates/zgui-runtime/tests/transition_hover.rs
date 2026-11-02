//! A hover rule that changes `transition` lays nothing out.
//!
//! Transitions tell the engine how a later change animates. Layout and paint never read them, so a
//! hover that only sets `transition: none` owes no layout, no new boxes and no shaping. A hover
//! that also moves a colour owes a repaint of that colour and nothing more. A hover that also moves
//! a length still lays the row out. The pixels of these frames are checked against a full repaint
//! in `zgui-ui`'s `transition_hover.rs`.
//!
//! It is its own test target because the counters are one process-wide block.

mod support;

use std::time::Duration;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_profile::Counter;
use zgui_runtime::Runtime;
use zgui_testkit_scene::counters::{Measurement, Recording};
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// How many rows the list holds.
const ROWS: usize = 40;

/// How tall a row is, in CSS pixels.
const ROW: f32 = 7.0;

/// The colour of a row, and of a row under the pointer.
const REST: u8 = 20;
const LIT: u8 = 200;

/// A list of rows with a label each, whose hover rule is `hover`.
fn list(hover: &str) -> Harness<Runtime> {
    let css = format!(
        "root {{ display: block; width: 400px; height: 300px }}
         .row {{ display: flex; height: {ROW}px; padding: 0 8px; font-size: 6px;
                 line-height: {ROW}px; background-color: rgb({REST}, {REST}, {REST});
                 transition: background-color 90ms }}
         .row:hover {{ {hover} }}
         .label {{ display: block }}"
    );
    let mut harness = support::app_with_text(&css, move |cx: &mut BuildCx<'_>| {
        let mut rows = zgui_elements::column();
        for index in 0..ROWS {
            rows = rows.child(
                zgui_elements::column().class("row").child(
                    zgui_elements::column()
                        .class("label")
                        .child(format!("row number {index}")),
                ),
            );
        }
        Box::new(rows.into_view().build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(16);
    harness
}

/// Moves the pointer to the middle of row `index` and runs every frame that asks for.
fn point_at(harness: &mut Harness<Runtime>, index: usize) {
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::<CssPx, Css>::new(
            CssPx(100.0),
            CssPx(ROW * index as f32 + ROW / 2.0),
        )),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);
}

/// Runs the clock past every transition the list holds.
fn finish_transitions(harness: &mut Harness<Runtime>) {
    for _ in 0..20 {
        harness.advance(Duration::from_millis(8));
        harness.pump();
    }
    harness.settle(8);
}

/// Moves the pointer down over every row, running each leave transition to its end.
fn sweep(harness: &mut Harness<Runtime>, recording: &mut Recording) -> Measurement {
    point_at(harness, 0);
    finish_transitions(harness);
    recording.measure(|| {
        for index in 1..ROWS {
            point_at(harness, index);
            finish_transitions(harness);
        }
    })
}

/// Asserts that the sweep laid nothing out, made no box and shaped no text.
fn assert_laid_out_nothing(swept: &Measurement) {
    assert!(
        swept.get(Counter::ElementsRestyled) >= ROWS as u64 - 1,
        "the hover has to restyle each row, or the sweep tests idle frames"
    );
    for counter in [
        Counter::NodesRelaidOut,
        Counter::BoxesRebuilt,
        Counter::TextShaped,
        Counter::TextRebroken,
        Counter::FragmentsRebuilt,
    ] {
        assert_eq!(swept.get(counter), 0, "the sweep counted {counter:?}");
    }
}

#[test]
fn a_hover_that_only_sets_transition_lays_out_and_paints_nothing() {
    let mut recording = Recording::begin();
    let mut harness = list("transition: none");
    let swept = sweep(&mut harness, &mut recording);
    assert_laid_out_nothing(&swept);
    assert_eq!(
        swept.get(Counter::Repaints),
        0,
        "nothing a row draws moved, so nothing is encoded again"
    );
    harness.shut_down();
}

#[test]
fn a_hover_that_sets_transition_and_a_colour_only_repaints_the_colour() {
    let mut recording = Recording::begin();
    let mut harness = list(&format!(
        "background-color: rgb({LIT}, {LIT}, {LIT}); transition: none"
    ));
    let swept = sweep(&mut harness, &mut recording);
    assert_laid_out_nothing(&swept);
    assert!(
        swept.get(Counter::Repaints) >= ROWS as u64 - 1,
        "each row the pointer reached changed colour"
    );
    harness.shut_down();
}

#[test]
fn a_hover_that_sets_transition_and_a_padding_still_lays_the_row_out() {
    let mut recording = Recording::begin();
    let mut harness = list("padding: 0 12px; transition: none");
    let swept = sweep(&mut harness, &mut recording);
    assert!(
        swept.get(Counter::NodesRelaidOut) > 0,
        "the padding moved, so the row is laid out again"
    );
    assert_eq!(
        swept.get(Counter::BoxesRebuilt),
        0,
        "a padding keeps the boxes"
    );
    harness.shut_down();
}
