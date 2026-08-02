//! An expired deadline produces exactly one frame, and the loop then goes back to sleep.
//!
//! A deadline arriving draws nothing by itself. The loop wakes, reports the arrival as the cause of
//! the turn, and stops there — nothing in a windowing library turns that into a request to draw. So
//! two opposite failures live at this one edge, and both look like nothing happening:
//!
//! * with no edge at all, the deadline arrives and no frame ever runs;
//! * with the edge but no clamp, the moment that has already passed stays installed, its arrival is
//!   re-derived on every iteration of the loop, and thousands of arrivals are reported while no
//!   frame runs at all.
//!
//! One arrival and one frame is the whole of what this asserts, and it is the assertion that fails
//! on both. The frame is counted from the arrival rather than from zero, because the platform asks
//! for a first paint of its own when a window is created and counting that would let it stand in
//! for the frame the deadline owed.

#[path = "support/mod.rs"]
mod support;

use std::time::{Duration, Instant};

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
};
use zgui_platform_winit::WinitApp;

/// How far ahead the frame is asked for.
const DELAY: Duration = Duration::from_millis(150);

/// How long the loop is watched afterwards, to see whether it goes back to sleep.
const SETTLE: Duration = Duration::from_millis(400);

/// An application that wants one frame, once, at a moment it names.
#[derive(Default)]
struct Probe {
    /// The moment it is waiting for, until a frame retires it.
    due: Option<Instant>,
    /// How many times it was told a deadline had arrived.
    arrivals: u32,
    /// How many frames have run since the first arrival.
    frames_after_arrival: u32,
}

impl AppHandler for Probe {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        cx.create_surface(&SurfaceAttributes::new("deadline"))
            .expect("a window could not be created");
        self.due = Some(cx.clock().now() + DELAY);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
        if !matches!(event, SurfaceEvent::RedrawRequested) || self.arrivals == 0 {
            return;
        }
        self.frames_after_arrival += 1;
        if self.frames_after_arrival == 1 {
            // The loop is then watched for a while: a correct park runs no further frames and
            // reports no further arrivals, and a spin does both thousands of times.
            support::wake_after(cx.waker(), SETTLE, || WakeReason::DeviceLost);
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, _reason: WakeReason) {
        cx.request_exit();
    }

    fn idle(&mut self, _cx: &dyn PlatformCx) -> IdlePolicy {
        // Reported without being clamped here on purpose: the clamp under test belongs to the
        // backend, and an application that had already applied it would leave the backend's own
        // version of it unexercised.
        self.due.map_or(IdlePolicy::Block, IdlePolicy::BlockUntil)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.arrivals += 1;
        // The arrival is what retires the work and asks for the frame that shows it. Both halves
        // matter: without the request there is no frame, and without retiring the work the moment
        // stays in the past for ever.
        self.due = None;
        for surface in cx.surfaces() {
            surface.request_redraw();
        }
    }
}

fn main() {
    const PROPERTY: &str = "an expired deadline produces exactly one frame and does not spin";

    let Some(event_loop) = support::event_loop() else {
        return;
    };
    support::watchdog(PROPERTY);

    let mut app = WinitApp::new(&event_loop, Probe::default());
    event_loop.run_app(&mut app).expect("the loop ran");

    let probe = app.handler();
    assert_eq!(
        probe.arrivals, 1,
        "the deadline arrived {} times; once is the whole budget",
        probe.arrivals
    );
    assert_eq!(
        probe.frames_after_arrival, 1,
        "the deadline arrived and produced {} frames, where exactly one was owed",
        probe.frames_after_arrival
    );
    assert_eq!(
        app.park().resumes(),
        1,
        "the platform reported {} arrivals for one deadline",
        app.park().resumes()
    );
    assert!(
        app.park().deadline().is_none(),
        "a deadline that has been serviced is still installed"
    );
    // A spin takes as many turns as the processor allows. Waiting takes one per thing that happens.
    let turns = app.turns();
    println!("the loop took {turns} turns to service one deadline and go back to sleep");
    assert!(
        turns < 64,
        "the loop took {turns} turns to service one deadline, which is a poll and not a park"
    );

    support::passed(PROPERTY);
}
