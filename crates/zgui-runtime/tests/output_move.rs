//! A window dragged from one output to another, and what it is paced and sized by afterwards.
//!
//! A desktop with two monitors on it rarely has two of the same monitor. One is 120 Hz at a ratio
//! of one, the next is 75 Hz at a ratio of 1.2, and a window that keeps the first one's numbers
//! after being dragged onto the second renders 1.44 times the pixels that output can show and owes
//! its frames at a rate no display is refreshing at. Neither is visible from inside the window:
//! every frame is correct, there are simply too many of them, drawn too large, and the ones that
//! reach the screen are the ones the compositor did not have to drop.
//!
//! So both numbers are read from the surface on the frame that uses them rather than remembered
//! from the one the window was opened on, and that is what is asserted here — the ratio through
//! the extent the document is laid out into, and the interval through the rate an animation
//! actually ticks at afterwards.

mod support;

use std::time::Duration;

use zgui_geom::{Device, DevicePx, Size};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};

/// A document that is exactly as large as the surface, and pulses for ever inside it.
///
/// Infinite deliberately: a cadence is counted over a span, and an animation that ended inside the
/// span would be counted partly against a window that had correctly stopped drawing.
const CSS: &str = "root { display: block; width: 100%; height: 100% }
                   @keyframes pulse { from { opacity: 1 } to { opacity: 0.4 } }
                   column { display: block; width: 100%; height: 100%;
                            background-color: rgb(200, 200, 200);
                            animation: pulse 2s linear infinite }";

/// The output the window is born on: 120 Hz at a ratio of one.
const BORN_ON: u32 = 120_000;

/// The output it is dragged to: 74.925 Hz at a ratio of 1.2.
const MOVED_TO: u32 = 74_925;

/// The surface, in device pixels, at each of the two ratios. Both are 400 by 300 CSS pixels.
const AT_ONE: Size<DevicePx, Device> = Size::new(DevicePx(400.0), DevicePx(300.0));
const AT_ONE_POINT_TWO: Size<DevicePx, Device> = Size::new(DevicePx(480.0), DevicePx(360.0));

/// One frame of an output refreshing `millihertz` times a second.
fn interval(millihertz: u32) -> Duration {
    zgui_platform::refresh_interval(Some(millihertz))
}

/// The extent of the root box's fragment, in device pixels.
fn root_extent(window: &zgui_runtime::Window) -> Size<DevicePx, Device> {
    let layout = window.layout().borrow();
    let root = layout.root().expect("the document has a root box");
    let fragment = *layout
        .fragments_of_box(root)
        .first()
        .expect("the root box produced a fragment");
    layout
        .fragment(fragment)
        .expect("the fragment is live")
        .border_box
        .size
}

/// Runs `span` of virtual time with an unrelated wake between ticks, and counts the frames.
///
/// The step is three sevenths of a refresh interval, so a little over two wakes land inside every
/// interval — which is the rate an ordinary window sees them at. A deadline derived from the
/// present moment rather than from the moment the last frame was owed at halves the frame rate at
/// that rate of wakes, so the count below is a measurement of both.
fn frames_over(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    millihertz: u32,
    span: Duration,
) -> u64 {
    let step = interval(millihertz) * 3 / 7;
    let steps = span.as_nanos() / step.as_nanos();
    harness.reset_counts();
    let mut frames = 0;
    for _ in 0..steps {
        harness.advance(step);
        harness.deliver_to_first(SurfaceEvent::Occluded(false));
        frames += harness.pump();
    }
    frames
}

/// Asserts that `frames` is one per refresh of a `millihertz` output over `span`.
fn assert_one_frame_per_refresh(frames: u64, millihertz: u32, span: Duration) {
    let refreshes = (span.as_nanos() / interval(millihertz).as_nanos()) as u64;
    // One either way: the span does not divide into a whole number of steps, and the last tick of
    // the run may be owed a moment after the run ends.
    assert!(
        frames + 1 >= refreshes && frames <= refreshes + 1,
        "a window on a {} hz output drew {frames} frames where the output showed {refreshes}",
        millihertz / 1_000
    );
}

#[test]
fn a_window_moved_between_outputs_adopts_the_new_refresh() {
    const SPAN: Duration = Duration::from_millis(500);
    let mut harness = support::app(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::column().class("root").into_view().build(cx))
    });
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(BORN_ON));
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: 1.0,
        size: AT_ONE,
    });
    harness.settle(16);

    assert_eq!(
        harness.app().windows()[0].refresh_interval(),
        interval(BORN_ON),
        "the window did not take the interval of the output it was born on"
    );
    assert_eq!(root_extent(&harness.app().windows()[0]), AT_ONE);
    assert_one_frame_per_refresh(frames_over(&mut harness, BORN_ON, SPAN), BORN_ON, SPAN);

    // The move. Both halves arrive as a window system delivers them: the surface is already on the
    // new output by the time anything is said about it, and what is said is the new ratio and the
    // extent the surface now has.
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(MOVED_TO));
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: 1.2,
        size: AT_ONE_POINT_TWO,
    });
    harness.settle(16);

    assert_eq!(
        harness.app().windows()[0].refresh_interval(),
        interval(MOVED_TO),
        "the window kept the interval of the output it was born on"
    );
    assert_eq!(
        root_extent(&harness.app().windows()[0]),
        AT_ONE_POINT_TWO,
        "the document was laid out for the ratio of the output the window came from"
    );
    assert_one_frame_per_refresh(frames_over(&mut harness, MOVED_TO, SPAN), MOVED_TO, SPAN);
    harness.assert_park_invariant();
}
