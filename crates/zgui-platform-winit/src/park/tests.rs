//! The properties the parking has to have, over a model of the loop's turn.
//!
//! Each of them is a way the loop has been got wrong, and each is stated as a count rather than as
//! a duration, because a count is exact on every machine and a duration is not. Several are
//! asserted again against the real loop, where the counts are the same and the arithmetic is the
//! platform's own.
//!
//! The last group is about the gap between the application deciding what it wants and the loop
//! installing it. Every property before them holds with that gap set to zero, which is what a
//! suite that never set it was measuring — and what let a loop that froze once in four hundred
//! thousand turns pass every one of them.

use std::sync::Arc;
use std::time::{Duration, Instant};

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
    Waker,
};

use crate::park::model::{Cause, Model, Reading};
use zgui_platform::Parked;

/// One tick of a sixty-hertz display, near enough for a model whose clock a test moves.
const TICK: Duration = Duration::from_millis(16);

/// An application that asks for a frame at a moment, and draws when it is given one.
///
/// Its `idle` reports the moment it is waiting for **without clamping it itself**. That is the
/// point: the clamp under test belongs to the backend, and an application that had already applied
/// it would leave the backend's own version unexercised.
#[derive(Default)]
struct Timed {
    /// When it wants a frame, if it wants one.
    due: Option<Instant>,
    /// How many frames it has been given.
    frames: u32,
    /// How many times it was told a deadline had arrived.
    arrivals: u32,
    /// How many wakes it was told about.
    wakes: u32,
    /// Whether the surface is hidden, in which case a frame retires nothing.
    occluded: bool,
    /// How far apart its ticks are, when it re-arms itself on every frame.
    interval: Option<Duration>,
    /// How many frames it declined to draw because the surface was hidden.
    skipped: u32,
}

impl AppHandler for Timed {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        cx.create_surface(&SurfaceAttributes::new("timed"))
            .expect("a surface over the model is always creatable");
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
        if !matches!(event, SurfaceEvent::RedrawRequested) {
            return;
        }
        if self.occluded {
            // The frame is entered and left without draining what it was asked for, which is what
            // a hidden window does: nothing is presented, and the work stays due.
            self.skipped += 1;
            return;
        }
        self.frames += 1;
        self.due = self.interval.map(|interval| cx.clock().now() + interval);
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.wakes += 1;
        for id in reason.surfaces() {
            if let Some(surface) = cx.surface(*id) {
                surface.request_redraw();
            }
        }
    }

    fn idle(&mut self, _cx: &dyn PlatformCx) -> IdlePolicy {
        self.due.map_or(IdlePolicy::Block, IdlePolicy::BlockUntil)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.arrivals += 1;
        for surface in cx.surfaces() {
            surface.request_redraw();
        }
    }
}

/// A model whose application wants one frame `after` from now.
fn waiting(after: Duration) -> Model<Timed> {
    let mut model = Model::new(Timed::default());
    let due = model.now() + after;
    model.app_mut().due = Some(due);
    model.turn();
    model
}

#[test]
fn a_signal_written_from_outside_any_input_event_still_produces_a_frame() {
    let mut model = Model::new(Timed::default());
    let surface = zgui_platform::Surface::id(
        model
            .platform()
            .offscreens()
            .first()
            .expect("the application created a surface")
            .as_ref(),
    );
    model.turn();
    assert_eq!(model.frames(), 0, "nothing had asked for a frame yet");

    // No pointer moved, no key was pressed and no deadline arrived. Something on another thread
    // finished, and that alone has to reach a parked loop.
    let waker: Arc<dyn Waker> = model.platform().waker();
    std::thread::spawn(move || {
        waker.wake(WakeReason::ReactiveWork {
            surfaces: Box::from([surface]),
        });
    })
    .join()
    .expect("the writing thread finished");

    assert_eq!(model.turn(), Cause::Awake, "the wake is what woke the loop");
    assert_eq!(model.app().wakes, 1);
    assert_eq!(model.frames(), 1, "the wake produced a frame");
    assert_eq!(model.app().frames, 1);
    model.assert_parks();
}

#[test]
fn work_belonging_to_one_surface_does_not_redraw_another() {
    let mut model = Model::new(Timed::default());
    model
        .platform()
        .create_surface(&SurfaceAttributes::new("second"))
        .expect("creatable");
    let first = zgui_platform::Surface::id(model.platform().offscreens()[0].as_ref());
    model.turn();

    model.platform().waker().wake(WakeReason::ReactiveWork {
        surfaces: Box::from([first]),
    });
    model.turn();
    assert_eq!(
        model.frames(),
        1,
        "an image decoding for one window is not a reason to redraw the other"
    );
}

#[test]
fn an_expired_deadline_produces_exactly_one_frame_and_does_not_spin() {
    let mut model = waiting(Duration::from_millis(700));
    assert!(
        matches!(model.parked(), Parked::Until(_)),
        "the loop parked on the moment it was asked to"
    );
    assert_eq!(model.frames(), 0, "nothing is drawn before the moment");
    assert_eq!(model.resumes(), 0);

    model.advance(Duration::from_millis(700));
    assert_eq!(model.turn(), Cause::Arrived);
    assert_eq!(model.resumes(), 1);
    assert_eq!(model.frames(), 1, "the arrival is what asked for the frame");
    assert_eq!(model.app().frames, 1);
    assert_eq!(model.parked(), Parked::Indefinitely);

    // The clock runs on for a further sixteen seconds of turns. A deadline that has been serviced
    // must never be reported again, and the loop must go back to sleep.
    model.run(1_000, TICK);
    assert_eq!(model.resumes(), 1, "the arrival was reported a second time");
    assert_eq!(model.frames(), 1, "one delay cost more than one frame");
    assert!(
        model.causes()[3..]
            .iter()
            .all(|cause| *cause == Cause::Idle),
        "the loop kept waking after the delay had been serviced"
    );
    model.assert_parks();
}

#[test]
fn an_idle_window_with_no_animation_parks_and_burns_no_cpu() {
    let mut model = Model::new(Timed::default());
    model.turn();
    model.run(1_000, TICK);

    assert_eq!(model.frames(), 0, "nothing changed, so nothing was drawn");
    assert_eq!(model.resumes(), 0, "nothing was waiting on a deadline");
    assert_eq!(model.parked(), Parked::Indefinitely);
    assert!(
        model.causes().iter().all(|cause| *cause == Cause::Idle),
        "an application with nothing to do woke the loop anyway"
    );
    model.assert_parks();
}

#[test]
fn a_moment_the_frame_cannot_retire_is_answered_every_turn_and_never_lost() {
    // This application keeps naming a moment that has already passed, because its surface is
    // hidden and the frame is entered and left without draining what it asked for. The runtime
    // never does this — a hidden window's animation and caret are excluded from the merged
    // deadline before the park is ever asked — so what is under test here is a handler that
    // ignores the contract, and the question is which way the loop fails when one does.
    //
    // The answer is that it must not fail silently. Every turn the moment is handed over and the
    // frame it asks for is requested, so the application gets exactly what it keeps asking for and
    // the loop can be seen doing it. What it must never do is park on nothing while holding the
    // moment, which is a window that stops answering with no counter anywhere showing why.
    let mut model = waiting(Duration::from_millis(700));
    model.app_mut().occluded = true;
    model.advance(Duration::from_millis(700));
    model.turn();
    model.run(1_000, TICK);

    assert!(
        model.app().arrivals > 1_000,
        "the moment was answered {} times over a thousand turns of being asked for",
        model.app().arrivals
    );
    assert_eq!(
        model.app().arrivals - model.app().skipped,
        1,
        "an arrival was reported that bought no frame, which is the spin; only the last one, \
         whose frame has not run yet, is allowed to be outstanding"
    );
    assert_eq!(
        model.parked(),
        Parked::Indefinitely,
        "and it never spun on a poll"
    );
    model.assert_parks();
}

#[test]
fn the_same_deadline_retires_and_keeps_ticking_when_the_surface_presents() {
    // The twin of the case above, and the reason the clamp is not simply "drop late deadlines":
    // a surface that *is* presenting retires the deadline on every frame and gets every tick.
    let mut model = Model::new(Timed::default());
    model.app_mut().interval = Some(TICK);
    let due = model.now() + TICK;
    model.app_mut().due = Some(due);
    model.turn();

    model.run(1_000, TICK);
    assert!(
        model.app().frames >= 999,
        "an animating surface was given {} frames over a thousand ticks",
        model.app().frames
    );
    model.assert_parks();
}

#[test]
fn installing_a_moment_that_has_passed_is_a_loop_that_never_draws() {
    // The positive control. Without it, every assertion above is one nobody has seen fail — and the
    // repair that closes the stall is exactly the repair that opens the spin, so the difference
    // between the two has to be demonstrated rather than asserted.
    let mut model = Model::misreading(Timed::default(), Reading::Unclamped);
    let due = model.now() + Duration::from_millis(700);
    model.app_mut().due = Some(due);
    model.app_mut().occluded = true;
    model.turn();

    model.advance(Duration::from_millis(700));
    model.run(1_000, TICK);
    assert!(
        model.resumes() >= 1_000,
        "the unclamped loop reported {} arrivals; if this is small the model has stopped \
         reproducing the behaviour the clamp exists for",
        model.resumes()
    );
    assert_eq!(
        model.app().frames,
        0,
        "the unclamped loop is a busy loop that runs no frames at all"
    );
}

/// How long the loop takes to get from asking the application to installing the answer.
///
/// Five microseconds is not a guess at a real one; it is chosen to sit in the middle of the
/// distances below, so that the same run has moments the gap swallows and moments it does not.
const SKEW: Duration = Duration::from_micros(5);

/// A model whose application wants one frame at a signed microsecond offset from now.
fn racing(ahead: i64, skew: Duration) -> Model<Timed> {
    let mut model = Model::new(Timed::default());
    model.set_skew(skew);
    let offset = Duration::from_micros(ahead.unsigned_abs());
    let due = if ahead < 0 {
        model.now() - offset
    } else {
        model.now() + offset
    };
    model.app_mut().due = Some(due);
    model
}

#[test]
fn a_moment_that_passes_while_the_loop_is_deciding_still_produces_its_frame() {
    // The boundary the loop actually froze on. The application picks its moment against one
    // reading of the clock and the loop installs it against a later one, so a moment a few
    // microseconds ahead is in the future when it is chosen and in the past when it is installed.
    // Below the gap and above it must both end in a frame; only the route differs.
    for ahead in [-1_000, -1, 0, 1, 3, 10, 30] {
        let mut model = racing(ahead, SKEW);
        for _ in 0..20 {
            model.turn();
        }
        assert_eq!(
            model.app().frames,
            1,
            "a moment {ahead}us away was never turned into a frame"
        );
        assert_eq!(
            model.app().arrivals,
            1,
            "a moment {ahead}us away was reported {} times",
            model.app().arrivals
        );
        assert_eq!(model.parked(), Parked::Indefinitely);
        model.assert_parks();
    }
}

#[test]
fn a_moment_swallowed_by_the_gap_is_paid_on_the_turn_that_swallowed_it() {
    // Not merely eventually. The frame is asked for within the same turn the moment was found to
    // have passed, so the very next turn draws it and no wake from anywhere else is needed.
    let mut model = racing(1, SKEW);
    assert_eq!(model.turn(), Cause::Idle, "nothing had happened yet");
    assert_eq!(
        model.app().arrivals,
        1,
        "the moment was handed over at once"
    );
    assert_eq!(model.app().frames, 0, "the frame it asked for is next");
    assert_eq!(
        model.turn(),
        Cause::Awake,
        "the request is what woke the loop"
    );
    assert_eq!(model.app().frames, 1);
    assert_eq!(model.resumes(), 1, "one moment, one arrival");
}

#[test]
fn a_moment_the_gap_does_not_reach_is_waited_for_as_usual() {
    let mut model = racing(30, SKEW);
    model.turn();
    assert_eq!(
        model.parked(),
        Parked::Until(model.app().due.expect("the application still wants it")),
        "a moment beyond the gap was paid early instead of waited for"
    );
    assert_eq!(model.app().arrivals, 0);
}

#[test]
fn a_moment_the_gap_swallowed_buys_a_frame_for_every_arrival_it_reports() {
    // The overdue path must not become the spin under another name. One arrival, one frame
    // requested, every time — the ratio the whole design is measured by.
    let mut model = racing(1, SKEW);
    model.app_mut().occluded = true;
    for _ in 0..1_000 {
        model.turn();
        model.assert_parks();
    }
    assert_eq!(
        model.app().arrivals - model.app().skipped,
        1,
        "an arrival was reported that bought no frame"
    );
    assert_eq!(model.parked(), Parked::Indefinitely);
}

#[test]
fn dropping_the_moment_is_what_stops_the_loop() {
    // The positive control for the failure this design exists to make unreachable, and the reason
    // every assertion above is one that has been seen to fail. The arithmetic here is what
    // shipped: a moment that has passed is refused, correctly, and then forgotten.
    let mut model = Model::misreading(Timed::default(), Reading::Dropping);
    model.set_skew(SKEW);
    let due = model.now() + Duration::from_micros(1);
    model.app_mut().due = Some(due);
    for _ in 0..1_000 {
        model.turn();
    }
    assert_eq!(
        model.app().frames,
        0,
        "the loop that drops the moment is supposed to freeze; if this draws, the model has \
         stopped reproducing the behaviour the design exists for"
    );
    assert_eq!(model.app().arrivals, 0, "nobody was ever told");
    assert_eq!(model.parked(), Parked::Indefinitely);
    assert!(
        model.causes().iter().all(|cause| *cause == Cause::Idle),
        "the frozen loop was woken by something"
    );
}
