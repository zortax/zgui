//! That a resize is treated as a level and never as a stream of events.
//!
//! A compositor reports what size the window *is*, not that a resize happened, and it reports it
//! again as fast as a pointer moves. The rules those two facts imply are the whole of this file:
//! the newest size is the only one that is ever built for; a size that is already superseded costs
//! no layout, no paint and no swapchain rebuild; and a window that has stopped being resized parks
//! rather than waking for a size it has already drawn.

mod support;

use std::time::Duration;

use zgui_geom::{Device, DevicePx, Size};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};

/// A root that is exactly as large as whatever contains it, so its fragment is the surface.
const CSS: &str = "root { display: block; width: 100%; height: 100% }";

/// An application whose only element fills the window.
fn app() -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::column().class("root").into_view().build(cx))
    })
}

/// The extent of the root box's fragment, in device pixels: what the frame was actually built for.
fn laid_out(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> Size<DevicePx, Device> {
    let window = &harness.app().windows()[0];
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

/// A surface extent of `width` by 600.
fn wide(width: f32) -> Size<DevicePx, Device> {
    Size::new(DevicePx(width), DevicePx(600.0))
}

/// The surface every event in these tests is delivered to.
fn only_surface(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> zgui_platform::SurfaceId {
    harness
        .platform()
        .offscreens()
        .first()
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window")
}

/// Puts the window's surface on an output refreshing at `millihertz`.
fn on_an_output_at(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    millihertz: u32,
) {
    harness
        .platform()
        .offscreens()
        .first()
        .expect("the application opened its window")
        .set_refresh_rate_millihertz(Some(millihertz));
}

/// How many times the window has pointed the renderer at a new extent.
fn swapchain_rebuilds(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> u64 {
    harness.app().windows()[0].surface_configures()
}

/// How many configures a `millis`-long drag at one configure per millisecond actually answered.
///
/// The drag a compositor really delivers: a configure per pointer sample, each carrying the redraw
/// the backend set on its own account, and one chance to draw between them. The clock is the
/// test's, because the question is how many of those chances were taken.
fn drag_answering(millis: u64, output: Option<u32>) -> u64 {
    let mut harness = app();
    let surface = only_surface(&harness);
    if let Some(millihertz) = output {
        on_an_output_at(&harness, millihertz);
    }
    harness.settle(8);
    harness.hold_clock(true);
    harness.redraw_on_configure(true);
    harness.advance(Duration::from_millis(100));
    let before = swapchain_rebuilds(&harness);

    for step in 0..millis {
        harness.deliver(surface, SurfaceEvent::Resized(wide(800.0 + step as f32)));
        let drawn = swapchain_rebuilds(&harness);
        harness.pump();
        if swapchain_rebuilds(&harness) > drawn {
            // A frame that ran during the drag is built for the size the window is *now*. This is
            // the whole of the user-visible complaint: a loop that drains a queue of configures
            // instead of sampling the level draws every intermediate size one refresh apart, so the
            // content is seen to arrive after the drag that produced it has finished.
            assert_eq!(
                laid_out(&harness),
                wide(800.0 + step as f32),
                "a frame during the drag was built for a size the window had already left"
            );
        }
        harness.advance(Duration::from_millis(1));
    }
    let answered = swapchain_rebuilds(&harness) - before;
    harness.shut_down();
    answered
}

#[test]
fn a_burst_of_configures_costs_one_frame_and_it_is_the_newest_size() {
    // The defect this exists for: answering every configure with a complete pipeline run. A drag
    // delivers a size per pointer sample, and a frame per sample is a frame per sample *presented*
    // — so the window draws a queue of sizes that are all already wrong, one per frame, and the
    // content is seen to arrive after the drag has finished. Only the last size in a burst can be
    // seen at all, so only the last one may be built.
    let mut harness = app();
    let surface = only_surface(&harness);
    harness.settle(8);
    // Far enough from the window's own first frame that the burst's first configure is admitted
    // where it arrives, exactly as the first sample of a drag is.
    harness.advance(Duration::from_millis(100));
    harness.reset_counts();

    let sizes: Vec<f32> = (0..24).map(|step| 700.0 + step as f32 * 8.0).collect();
    harness.deliver_all(
        surface,
        sizes
            .iter()
            .map(|width| SurfaceEvent::Resized(wide(*width))),
    );
    let frames = harness.settle(8);

    assert_eq!(
        frames, 1,
        "twenty-four configures inside one frame of the output bought {frames} pipeline runs; \
         every one but the last draws a size that is superseded before it can be scanned out"
    );
    assert_eq!(
        laid_out(&harness),
        wide(*sizes.last().expect("the burst is not empty")),
        "the frame was built for a size the window had already left"
    );
    assert_eq!(
        harness.app().windows()[0].deferred_resizes(),
        0,
        "a burst that arrives before any of it has been drawn needs no pacing to collapse: the \
         frame that has already been asked for is the one that draws the newest size, and a \
         deferral here would mean the pacing is standing in for coalescing that already works"
    );
    harness.shut_down();
}

#[test]
fn the_first_configure_after_a_quiet_window_is_answered_where_it_arrives() {
    // The other half, and the one that a pacing bug turns into a window that feels sticky: a
    // single resize must not wait for anything at all. The pacing measures from a resize frame
    // that has already run, so a window nobody has been dragging has nothing to wait behind.
    let mut harness = app();
    let surface = only_surface(&harness);
    harness.settle(8);
    harness.advance(Duration::from_millis(100));
    harness.reset_counts();

    harness.deliver(surface, SurfaceEvent::Resized(wide(1000.0)));
    assert_eq!(
        harness.redraws_requested(),
        1,
        "an isolated configure was made to wait for a deadline"
    );
    assert_eq!(harness.settle(8), 1);
    assert_eq!(laid_out(&harness), wide(1000.0));
    harness.shut_down();
}

#[test]
fn configures_arriving_after_a_resize_frame_wait_for_the_next_one_and_are_still_drawn() {
    // The pacing itself, and the stall it could become. Once a resize frame has run, the
    // configures that arrive before the output could have shown it are answered with a deadline
    // rather than with a pipeline run each — which is only correct if something crosses that
    // deadline. Nothing does if it is never installed, and the window then stops resizing until
    // some unrelated event happens to ask for a frame.
    let mut harness = app();
    let surface = only_surface(&harness);
    harness.settle(8);
    harness.advance(Duration::from_millis(100));
    harness.reset_counts();

    // One resize frame runs, which is what the next configures are paced against.
    harness.deliver(surface, SurfaceEvent::Resized(wide(900.0)));
    assert_eq!(harness.settle(8), 1);

    // Everything from here lands inside that frame's own refresh interval.
    let sizes: Vec<f32> = (0..12).map(|step| 1000.0 + step as f32 * 10.0).collect();
    harness.deliver_all(
        surface,
        sizes
            .iter()
            .map(|width| SurfaceEvent::Resized(wide(*width))),
    );
    let frames = harness.settle(8);

    assert_eq!(
        harness.app().windows()[0].deferred_resizes(),
        12,
        "the configures inside the interval each bought a pipeline run of their own"
    );
    assert_eq!(frames, 1, "the deferred configures bought {frames} frames");
    assert_eq!(
        laid_out(&harness),
        wide(*sizes.last().expect("the burst is not empty")),
        "the deferred configures were never drawn: the deadline that owes them was not installed"
    );
    assert!(
        harness.parked_deadline().is_none(),
        "the window is still asking to be woken for a resize it has already drawn"
    );
    harness.assert_park_invariant();
    harness.shut_down();
}

#[test]
fn a_drag_costs_one_frame_per_frame_of_the_output_however_fast_the_configures_arrive() {
    // The measurement the user made: content that arrives after the drag has finished, on a slow
    // output and not on a fast one. A configure arrives per pointer sample and each one used to be
    // answered by a complete pipeline run — layout, a full-surface repaint, and a swapchain
    // rebuild that waits for the device to go idle — so the loop spent its whole time drawing
    // sizes that were superseded before they could be scanned out.
    //
    // Each turn here is one configure and one chance to draw, which is what a drag actually
    // delivers, and the clock is held so that the turns are the test's rather than the harness's.
    let mut harness = app();
    let surface = only_surface(&harness);
    harness.settle(8);
    harness.hold_clock(true);
    harness.advance(Duration::from_millis(100));
    harness.reset_counts();

    // A hundred configures a millisecond apart: six refresh intervals of the fallback output.
    let mut frames = 0;
    for step in 0..100 {
        harness.deliver(surface, SurfaceEvent::Resized(wide(800.0 + step as f32)));
        frames += harness.pump();
        harness.advance(Duration::from_millis(1));
    }
    let interval = zgui_platform::refresh_interval(None).as_secs_f64();
    let ceiling = (0.100 / interval).ceil() as u64 + 1;

    assert!(
        frames <= ceiling,
        "a hundred configures over a hundred milliseconds bought {frames} pipeline runs; at one \
         frame of the output each, {ceiling} is everything that could have been seen"
    );
    assert!(frames > 0, "the resize was never drawn at all");
    // And what it did draw is the newest size each time, never a queue of stale ones.
    harness.advance(zgui_platform::refresh_interval(None));
    harness.pump();
    assert_eq!(
        laid_out(&harness),
        wide(899.0),
        "the window drew a size it had already left behind"
    );
    harness.assert_park_invariant();
    harness.shut_down();
}

#[test]
fn the_redraws_a_backend_produces_for_superseded_configures_cost_no_swapchain_rebuilds() {
    // The half of the chain no other test here can reach, and the one that carries the whole fix on
    // a real compositor. A windowing backend does not wait to be asked: winit's Wayland loop sets
    // the window's redraw flag on the same turn it reports the new size, so *deciding* that a
    // configure is too soon to answer saves nothing by itself — the frame is offered anyway, and
    // only declining it keeps the layout, the repaint and above all the swapchain rebuild from
    // running. That rebuild waits for the graphics device to go completely idle, which is why it is
    // counted here rather than frames: it is the cost that does not shrink when the step is small.
    let answered = drag_answering(60, None);
    let interval = zgui_platform::refresh_interval(None).as_secs_f64();
    let ceiling = (0.060 / interval).ceil() as u64 + 1;

    assert!(
        answered <= ceiling,
        "sixty configures over sixty milliseconds rebuilt the swapchain {answered} times; at one \
         frame of the output each, {ceiling} is everything that could have been seen"
    );
    assert!(answered > 0, "the resize was never drawn at all");
}

#[test]
fn a_faster_output_answers_more_of_the_same_drag_than_a_slower_one() {
    // That the pacing is read from the output the window is on, and is not a constant. Everything
    // above reads the same on a sixty-hertz assumption as on the truth, so a window that ignored
    // its surface entirely would satisfy the whole of the rest of this file — while capping a
    // two-hundred-and-forty-hertz display at a quarter of the resize frames it can show, and
    // holding a seventy-five-hertz one to frames it cannot.
    let slow = drag_answering(60, Some(75_000));
    let fast = drag_answering(60, Some(240_000));

    assert!(
        fast > slow * 2,
        "the same drag bought {slow} answered configures at 75 Hz and {fast} at 240 Hz; the two \
         outputs are more than three refresh intervals apart, so a window reading its own surface \
         cannot answer them at nearly the same rate"
    );
    assert!(slow > 0 && fast > 0, "the resize was never drawn at all");
}

#[test]
fn a_window_that_has_stopped_being_resized_runs_no_frames_at_all() {
    // The anti-spin half. The resize deadline exists only while a reconfiguration is owed; one
    // that outlives the frame that discharged it is a loop that wakes for ever and draws nothing,
    // which is the failure this backend was written to make visible.
    let mut harness = app();
    let surface = only_surface(&harness);
    harness.deliver(surface, SurfaceEvent::Resized(wide(1024.0)));
    harness.settle(8);
    harness.reset_counts();

    let frames = harness.run_for(Duration::from_secs(2), Duration::from_millis(8));
    assert_eq!(frames, 0, "an idle window drew {frames} frames");
    assert_eq!(harness.resumes(), 0);
    harness.assert_park_invariant();
    harness.shut_down();
}

#[test]
fn a_configure_that_repeats_the_extent_asks_for_nothing_at_all() {
    // Kept beside the pacing because the two are easy to confuse and only one of them is free: a
    // repeat is dropped outright and never becomes a deferred configure waiting on a deadline.
    let mut harness = app();
    let surface = only_surface(&harness);
    harness.deliver(surface, SurfaceEvent::Resized(wide(880.0)));
    harness.settle(8);
    harness.advance(Duration::from_millis(100));
    harness.reset_counts();

    harness.deliver(surface, SurfaceEvent::Resized(wide(880.0)));
    assert_eq!(harness.redraws_requested(), 0);
    assert_eq!(harness.settle(8), 0);
    assert_eq!(
        harness.app().windows()[0].deferred_resizes(),
        0,
        "a repeat of the extent the surface already has was counted as a deferred configure"
    );
    harness.shut_down();
}
