//! A moment the frame cannot retire is answered every turn and never lost.
//!
//! An occluded window still runs its frames — a timer behind a minimised window must still fire —
//! but the frame presents nothing and leaves without draining what it was asked for. The moment it
//! was waiting for therefore stays in the past, and the application keeps naming that same moment
//! on every turn it is asked. The runtime above this never does that; a hidden window's animation
//! and caret are dropped from the merged deadline before the loop is ever asked. So what runs here
//! is a handler that ignores the contract, and the question is which way the loop fails when one
//! does.
//!
//! There are two ways, and one of them is silent. Installed as asked, a moment that has already
//! passed yields no remaining time on every iteration of the loop and is re-derived as an arrival
//! every time: thousands of arrivals are reported, **no frame runs**, and a core burns while the
//! application looks completely idle. Refused *and forgotten*, the loop blocks with nothing left
//! that will ever ask for the frame, and the window stops answering with no counter anywhere
//! showing why — which is the failure that froze three campaigns of the gallery.
//!
//! What must happen instead is that the moment is handed over and the frame it asks for is
//! requested, every time it is named. The application then gets exactly what it keeps asking for,
//! and the number that says so is the ratio: **one frame for every arrival reported**. That ratio
//! is what separates this from the spin, and it is what this asserts.

#[path = "support/mod.rs"]
mod support;

use std::time::{Duration, Instant};

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
};
use zgui_platform_winit::WinitApp;

/// How far ahead the work is due.
const DELAY: Duration = Duration::from_millis(150);

/// How long the loop is watched after the moment has passed unretired.
const SETTLE: Duration = Duration::from_millis(600);

/// An application whose frames cannot retire the moment they were woken for.
#[derive(Default)]
struct Probe {
    /// The moment the work is due. Nothing ever retires it.
    due: Option<Instant>,
    /// How many times a deadline was reported as having arrived.
    arrivals: u32,
    /// How many frames were entered and left without presenting anything, after the arrival.
    skipped_after_arrival: u32,
}

impl AppHandler for Probe {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        cx.create_surface(&SurfaceAttributes::new("occluded"))
            .expect("a window could not be created");
        self.due = Some(cx.clock().now() + DELAY);
        support::wake_after(cx.waker(), DELAY + SETTLE, || WakeReason::DeviceLost);
    }

    fn surface_event(&mut self, _cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
        if !matches!(event, SurfaceEvent::RedrawRequested) || self.arrivals == 0 {
            return;
        }
        // The window is hidden, so the frame exits without presenting — and, crucially, without
        // draining the work that asked for it. The moment stays due for ever.
        self.skipped_after_arrival += 1;
    }

    fn wake(&mut self, cx: &dyn PlatformCx, _reason: WakeReason) {
        cx.request_exit();
    }

    fn idle(&mut self, _cx: &dyn PlatformCx) -> IdlePolicy {
        // Reported unclamped, on every turn, for ever. This is the adversarial input, and clamping
        // it here instead would leave the backend's own answer to it untested.
        self.due.map_or(IdlePolicy::Block, IdlePolicy::BlockUntil)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.arrivals += 1;
        for surface in cx.surfaces() {
            surface.request_redraw();
        }
    }
}

fn main() {
    const PROPERTY: &str = "a moment the frame cannot retire is answered every turn and never lost";

    let Some(event_loop) = support::event_loop() else {
        return;
    };
    support::watchdog(PROPERTY);

    let mut app = WinitApp::new(&event_loop, Probe::default());
    event_loop.run_app(&mut app).expect("the loop ran");

    let probe = app.handler();
    assert!(
        probe.arrivals > 1,
        "the moment was reported {} times: a moment that keeps being named and is not installed \
         has to keep being answered, or nothing will ever ask for the frame it wants",
        probe.arrivals
    );
    // On macOS, AppKit coalesces a redraw request that lands while one is already queued, so a
    // small fraction of arrivals buy no frame of their own. The spin this guards against buys
    // none at all, so one percent of slack cannot hide it.
    #[cfg(target_os = "macos")]
    let slack = 1 + probe.arrivals / 100;
    #[cfg(not(target_os = "macos"))]
    let slack = 1;
    assert!(
        probe.arrivals.abs_diff(probe.skipped_after_arrival) <= slack,
        "{} arrivals bought {} frames; an arrival that buys no frame is the spin, and only the \
         last one, whose frame has not run yet, may be outstanding",
        probe.arrivals,
        probe.skipped_after_arrival
    );
    assert_eq!(
        app.park().resumes(),
        u64::from(probe.arrivals),
        "the platform counted {} arrivals and the application was told about {}",
        app.park().resumes(),
        probe.arrivals
    );
    assert!(
        app.park().deadline().is_none(),
        "a moment that has already passed was installed as something to wait for"
    );
    let turns = app.turns();
    println!(
        "the occluded loop took {turns} turns and drew {} frames for {} arrivals",
        probe.skipped_after_arrival, probe.arrivals
    );
    assert!(
        u64::from(probe.arrivals) * 4 > turns,
        "the loop took {turns} turns for {} arrivals, so most of them did nothing at all",
        probe.arrivals
    );

    support::passed(PROPERTY);
}
