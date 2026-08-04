//! The frame-time chart records the frames it says it does, and still lets the window idle.
//!
//! It is the one sample taken on *every* frame rather than on the cadence, because a chart of
//! frame times whose point is the spike cannot be built from one frame in thirty. What keeps that
//! from being the runaway the timeline was, is that only the *reading* is per frame: the vector it
//! accumulates into is not a signal, and nothing is published until the cadence comes round.
//!
//! A target of its own because the latency ring is process-global, and a second window in the same
//! binary would be writing into the frames this one is counting.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::{DevTools, Tab};

use support::{frames_over, opened, run, text};

/// The chart fills up as frames run, and says what the worst of them cost.
#[test]
fn the_chart_records_the_frames_that_ran() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.show(Tab::Timeline);

    // Long enough for several publications of the cadence, so the chart has a history behind it.
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
        !shown.contains("worst of them0.0 us"),
        "every recorded frame took no time at all: {shown:.600}"
    );
}

/// Recording a frame per frame still leaves a still document idle.
///
/// The property the accumulation exists to protect. A chart published from every frame would be a
/// signal written from every frame, which is a panel redrawn every frame, which is a window that
/// never settles for as long as the tab is open.
#[test]
fn the_chart_does_not_keep_the_window_awake() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.show(Tab::Timeline);
    run(&mut harness, 200);

    let frames = frames_over(&mut harness, 300);
    assert_eq!(
        frames, 0,
        "the frame-time chart woke the window {frames} times over 300 vsyncs on a still document"
    );
}

/// The graph takes the width the panel gives it, and stands beside a readable scale.
#[test]
fn the_graph_fills_the_panel_and_says_what_its_height_means() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.show(Tab::Timeline);
    run(&mut harness, 200);

    let panel = support::box_of(&harness, "zgui-devtools");
    let graph = support::find_box(&harness, "zgui-devtools__graph").expect("the graph is drawn");
    let axis = support::find_box(&harness, "zgui-devtools__axis").expect("the scale is drawn");

    // Everything the panel has left after its padding, the scale and the scrollbar gutter.
    assert!(
        graph.size.width.0 > panel.size.width.0 - axis.size.width.0 - 60.0,
        "the graph is {}px wide in a {}px panel beside a {}px scale",
        graph.size.width.0,
        panel.size.width.0,
        axis.size.width.0
    );
    // And it cannot push the tab sideways.
    assert!(
        graph.size.width.0 <= panel.size.width.0,
        "the graph is wider than the panel it is in"
    );
    assert!(
        text(&harness).contains("ms"),
        "the scale beside the graph is not in readable units"
    );
}
