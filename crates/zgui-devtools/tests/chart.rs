//! The frame-time graph: what it records, how it is drawn, and what it costs.
//!
//! It is the one sample taken on *every* frame rather than on the cadence, because a graph of frame
//! times whose point is the spike cannot be built from one frame in thirty. What keeps that from
//! being a runaway is that only the *reading* is per frame: the vector it accumulates into is not a
//! signal, and nothing is published until the cadence comes round.
//!
//! **One test, one window.** The latency ring is process-global, so a second window in this binary
//! would write into the frames this one is counting — and the graph would be built from two windows'
//! frames interleaved. Every assertion here is therefore made against one window in one run.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::{DevTools, Tab};

use support::{box_of, find_box, frames_over, opened, run, text};

/// The graph records the frames that ran, fills the panel, and lets the window idle.
#[test]
fn the_graph_records_frames_fills_the_panel_and_leaves_the_window_idle() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.show(Tab::Timeline);

    // Long enough for several publications of the cadence, so the graph has a history behind it.
    run(&mut harness, 200);

    let shown = text(&harness);
    assert!(
        shown.contains("frame by frame"),
        "the graph is not in the timeline tab: {shown:.600}"
    );
    assert!(
        shown.contains("worst of them"),
        "the graph does not report the worst frame: {shown:.600}"
    );
    // A recorded frame is a real duration, so the worst of them is not zero.
    assert!(
        !shown.contains("worst of them0.0 ms"),
        "every recorded frame took no time at all: {shown:.600}"
    );
    // The scale beside the graph is what turns a peak into a number.
    assert!(
        shown.contains("ms"),
        "the scale beside the graph is not in readable units: {shown:.600}"
    );

    let panel = box_of(&harness, "zgui-devtools");
    let graph = find_box(&harness, "zgui-devtools__graph").expect("the graph is drawn");
    let axis = find_box(&harness, "zgui-devtools__axis").expect("the scale is drawn");

    // Everything the panel has left after its padding, the scale and the scrollbar gutter.
    assert!(
        graph.size.width.0 > panel.size.width.0 - axis.size.width.0 - 60.0,
        "the graph is {}px wide in a {}px panel beside a {}px scale",
        graph.size.width.0,
        panel.size.width.0,
        axis.size.width.0
    );
    assert!(
        graph.size.width.0 <= panel.size.width.0,
        "the graph is wider than the panel it is in"
    );

    // A graph published from every frame would be a signal written from every frame, which is a
    // panel redrawn every frame, which is a window that never settles while the tab is open.
    let frames = frames_over(&mut harness, 300);
    assert_eq!(
        frames, 0,
        "the frame-time graph woke the window {frames} times over 300 vsyncs on a still document"
    );
}
