//! The wake path, which is the part of the loop with two opposite failure modes that look alike.
//!
//! A window that never draws when a signal is written from outside an input event is *stalled*. A
//! window that reports a deadline reached on every turn while running no frames is *spinning*.
//! Nothing about either is visible in what is on the screen — in both cases it is the last frame —
//! so each one is asserted on directly here.

mod support;

use std::rc::Rc;
use std::time::Duration;

use zgui_reactive::prelude::{Get, GetUntracked, Set};
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// A view whose text is whatever `count` says.
fn counting(
    count: zgui_reactive::RwSignal<i32>,
) -> impl FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> {
    move |cx| {
        let view = zgui_elements::column()
            .class("root")
            .child(zgui_elements::text().child(move || count.get().to_string()));
        Box::new(view.into_view().build(cx))
    }
}

/// The sheet every fixture here is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block }
                   text { display: block }";

#[test]
fn a_signal_written_outside_any_input_event_still_produces_a_frame() {
    // The stall. With no wake edge the write marks work ready and then waits for the user to move
    // the mouse — an async spinner that spins for ever until something else happens to cause a
    // frame.
    let count = zgui_reactive::RwSignal::new(0);
    let mut harness = support::app(CSS, counting(count));
    harness.settle(8);
    harness.reset_counts();

    count.set(1);

    assert!(
        harness.platform().has_pending_wakes(),
        "writing a signal from outside an input event reached the platform with nothing at all; \
         the loop would sleep until the user moved the mouse"
    );
    let frames = harness.settle(8);
    assert!(frames >= 1, "the wake produced no frame");
    assert!(
        harness.redraws_requested() >= 1,
        "the wake was delivered and asked for nothing"
    );
}

#[test]
fn a_write_from_another_thread_reaches_the_loop_as_a_wake() {
    // The same edge, taken the way it is actually taken in an application: the work finishes
    // somewhere that is not the user-interface thread, and the ping is what the loop hears.
    // A value the view is displaying, which is the only kind whose arrival has to reach the loop.
    let count = zgui_reactive::ArcRwSignal::new(0);
    let read = count.clone();
    let mut harness = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let read = read.clone();
        let view = zgui_elements::column()
            .class("root")
            .child(zgui_elements::text().child(move || read.get().to_string()));
        Box::new(view.into_view().build(cx))
    });
    harness.settle(8);
    harness.reset_counts();
    assert!(
        !harness.platform().has_pending_wakes(),
        "the loop starts this parked with nothing owed"
    );

    // `set` on a signal shared with a worker thread reaches the platform waker synchronously,
    // inside the write, on the writing thread.
    let sent = count.clone();
    std::thread::spawn(move || sent.set(7))
        .join()
        .expect("joined");

    assert!(
        harness.platform().has_pending_wakes(),
        "the write happened on a thread that is not the loop's, and the loop was told nothing; it \
         would sleep on until something else woke it"
    );
    let frames = harness.settle(8);
    assert!(frames >= 1, "the wake produced no frame");
    assert!(
        harness.redraws_requested() >= 1,
        "the wake was delivered and asked for nothing"
    );
    assert_eq!(count.get_untracked(), 7, "the write landed on the signal");
}

#[test]
fn a_timer_costs_exactly_one_wake_and_one_frame_and_leaves_no_deadline() {
    // The three assertions pin the deadline edge rather than the delay. The first fails on a
    // stall, the third on the zero-timeout spin, and the second on an in-frame requester that
    // asks the platform directly instead of letting the frame's last phase ask once.
    let fired = Rc::new(std::cell::Cell::new(0u32));
    let count = zgui_reactive::RwSignal::new(0);
    let scheduled = Rc::clone(&fired);
    let mut harness = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let fired = Rc::clone(&scheduled);
        // Held for the life of the window: dropping the handle cancels the callback.
        let handle = zgui_view::set_timeout(Duration::from_millis(700), move || {
            fired.set(fired.get() + 1);
            count.set(1);
        });
        core::mem::forget(handle);
        Box::new(zgui_elements::column().into_view().build(cx))
    });
    harness.settle(8);

    assert_eq!(
        harness.parked_deadline().map(|at| at - harness.now()),
        Some(Duration::from_millis(700)),
        "the loop parked on the callback's deadline"
    );
    harness.reset_counts();

    harness.advance(Duration::from_millis(700));
    assert_eq!(
        harness.redraws_requested(),
        1,
        "the reached deadline itself is what asked for the frame"
    );
    assert_eq!(harness.pump(), 1, "and it produced exactly one frame");
    assert_eq!(harness.frames_requested(), 1);
    assert_eq!(fired.get(), 1, "the callback ran, once");
    assert!(
        harness.parked_deadline().is_none(),
        "an expired deadline was re-installed, which is the spin"
    );
    harness.assert_park_invariant();
}

#[test]
fn an_idle_window_parks_and_draws_nothing_for_ten_seconds() {
    // Mounted and substantial, because an empty document is idle whatever the loop does with it.
    let mut harness = support::app(CSS, |cx: &mut BuildCx<'_>| {
        let mut view = zgui_elements::column().class("root");
        for _ in 0..1_000 {
            view = view.child(
                zgui_elements::column()
                    .child(zgui_elements::text().child("row"))
                    .child(zgui_elements::text().child("cell")),
            );
        }
        Box::new(view.into_view().build(cx))
    });
    harness.settle(8);
    harness.reset_counts();

    let frames = harness.run_for(Duration::from_secs(10), Duration::from_millis(16));

    assert_eq!(frames, 0, "an idle window drew {frames} frames");
    assert_eq!(
        harness.resumes(),
        0,
        "an idle window was woken by a deadline"
    );
    assert!(harness.parked_deadline().is_none());
    harness.assert_park_invariant();
}

#[test]
fn resumes_never_outrun_frames_over_a_scripted_run() {
    // The one assertion that separates a correct park from a zero-timeout spin that runs no frames
    // at all. It is checked after every advance and every turn inside the harness; this run is
    // what makes it a run rather than a claim.
    let fired = Rc::new(std::cell::Cell::new(0u32));
    let scheduled = Rc::clone(&fired);
    let mut harness = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let fired = Rc::clone(&scheduled);
        let handle = zgui_view::set_interval(Duration::from_millis(50), move || {
            fired.set(fired.get() + 1);
        });
        core::mem::forget(handle);
        Box::new(zgui_elements::column().into_view().build(cx))
    });
    harness.settle(8);
    harness.reset_counts();

    for _ in 0..20 {
        harness.advance(Duration::from_millis(50));
        harness.pump();
    }

    assert_eq!(fired.get(), 20, "every interval tick fired exactly once");
    assert!(
        harness.resumes() <= harness.frames_requested() + 1,
        "{} resumes against {} frames",
        harness.resumes(),
        harness.frames_requested()
    );
}

#[test]
fn a_callback_that_schedules_the_next_one_still_gets_its_frame() {
    // The stall's other half: a deadline that is already in the past when the loop parks. Nothing
    // recomputes the park outside a frame, so a deadline dropped here is dropped for ever.
    let ticks = Rc::new(std::cell::Cell::new(0u32));
    let outer = Rc::clone(&ticks);
    let mut harness = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let ticks = Rc::clone(&outer);
        let handle = zgui_view::set_timeout(Duration::from_millis(700), move || {
            ticks.set(ticks.get() + 1);
            // Scheduled from inside a callback, for no delay at all: the "next tick" a debounce
            // and a two-stage measurement both need.
            let ticks = Rc::clone(&ticks);
            core::mem::forget(zgui_view::set_timeout(Duration::ZERO, move || {
                ticks.set(ticks.get() + 1);
            }));
        });
        core::mem::forget(handle);
        Box::new(zgui_elements::column().into_view().build(cx))
    });
    harness.settle(8);
    harness.advance(Duration::from_millis(700));
    harness.settle(8);

    assert_eq!(
        ticks.get(),
        2,
        "the callback the first one scheduled never ran"
    );
}
