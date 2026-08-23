//! An idle window with no animation parks, and burns nothing.
//!
//! This is the property the whole design rests on: an interface that is not changing must cost
//! nothing. It is also the one no assertion about *time* can establish, because a machine under
//! load and a machine at rest answer the same question differently. So it is asserted as counts —
//! frames run, deadline arrivals reported, turns of the loop taken — every one of which a spinning
//! loop inflates by orders of magnitude and a parked one leaves at rest.
//!
//! The quiet is measured between two signals rather than from the start, because opening a window
//! is not nothing: the compositor configures it, the application is told its size, and a first
//! frame is asked for. What has to be zero is what happens *after* all that.

#[path = "support/mod.rs"]
mod support;

use std::time::Duration;

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
};

/// How long the window is left to finish being opened before the quiet starts.
const SETTLE: Duration = Duration::from_millis(400);

/// How long the loop is then left with nothing whatever to do.
const QUIET: Duration = Duration::from_millis(600);

/// An application that opens a window and then wants nothing at all.
#[derive(Default)]
struct Probe {
    /// How many frames have run in total.
    frames: u32,
    /// How many signals have arrived: one starts the quiet, the next ends it.
    signals: u32,
    /// How many frames had run when the quiet began.
    frames_at_quiet: Option<u32>,
}

impl AppHandler for Probe {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        cx.create_surface(&SurfaceAttributes::new("idle"))
            .expect("a window could not be created");
        support::wake_after(cx.waker(), SETTLE, || WakeReason::DeviceLost);
    }

    fn surface_event(&mut self, _cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
        if matches!(event, SurfaceEvent::RedrawRequested) {
            self.frames += 1;
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, _reason: WakeReason) {
        self.signals += 1;
        if self.signals == 1 {
            self.frames_at_quiet = Some(self.frames);
            support::wake_after(cx.waker(), QUIET, || WakeReason::DeviceLost);
        } else {
            cx.request_exit();
        }
    }

    fn idle(&mut self, _cx: &dyn PlatformCx) -> IdlePolicy {
        // Nothing is animating and no timer is set, so there is nothing to be woken for.
        IdlePolicy::Block
    }
}

fn main() {
    const PROPERTY: &str = "an idle window with no animation parks and burns no cpu";

    support::watchdog(PROPERTY);
    let Some(mut app) = support::loop_for(PROPERTY, Probe::default()) else {
        return;
    };
    app.run().expect("the loop ran");

    let probe = app.handler();
    assert_eq!(
        probe.signals, 2,
        "the quiet was never started or never ended"
    );
    let at_quiet = probe
        .frames_at_quiet
        .expect("the quiet started, so this was recorded with it");
    assert_eq!(
        probe.frames,
        at_quiet,
        "{} frames ran while nothing at all was changing",
        probe.frames - at_quiet
    );
    assert_eq!(
        app.park().resumes(),
        0,
        "nothing was waiting on a deadline and {} arrivals were reported",
        app.park().resumes()
    );
    assert!(
        app.park().deadline().is_none(),
        "an idle loop parked on a deadline it never asked for"
    );
    // Most of a second of doing nothing. A loop that polls instead of blocking takes six figures of
    // turns in that time; one that blocks takes the handful the compositor had something to say on.
    let turns = app.turns();
    assert!(
        turns < 64,
        "the loop took {turns} turns while doing nothing at all, which is a poll and not a park"
    );

    support::passed(PROPERTY, &format!("{turns} turns over {QUIET:?} of quiet"));
}
