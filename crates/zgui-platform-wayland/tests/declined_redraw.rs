//! A declined redraw costs the loop nothing, and the frame after it arrives at once.
//!
//! An application may refuse a redraw it is offered — the runtime does exactly that for every
//! configure its resize pacing defers — and a refusal is not a frame: nothing ran, nothing was
//! committed, and the compositor is owed nothing. The next request must therefore be answered in
//! the turn it is made, from the loop's own deadline, with no compositor round trip in front of
//! it.
//!
//! The failure this pins against: a declined redraw that still ends in a bufferless commit leaves
//! a frame callback owed, and the frame the refusal deferred then waits behind the compositor's
//! answer to an empty commit — or, on a compositor that never answers those, behind the watchdog's
//! whole grace. Interleaving a decline before every drawn frame makes that cost land on every
//! frame of this loop, so a loop that finishes promptly is the property.

#[path = "support/mod.rs"]
mod support;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, PlatformError, SurfaceEvent, SurfaceId, WakeReason,
};
use zgui_platform_wayland::WaylandApp;

/// How many frames the application draws before the loop finishes.
const FRAMES: u64 = 8;

/// How many redraws were declined along the way.
static DECLINED: AtomicU64 = AtomicU64::new(0);

/// An application that refuses every second redraw and asks again at once.
///
/// The first redraw is always forwarded — it is the one that maps the surface — and from then on
/// declines alternate with drawn frames, which is the runtime's own cadence when a drag delivers
/// configures faster than the output can show them.
struct DeclineEveryOther<A: AppHandler> {
    /// The application.
    inner: A,
    /// How many redraws have been delivered.
    delivered: u64,
}

impl<A: AppHandler> AppHandler for DeclineEveryOther<A> {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        if !matches!(event, SurfaceEvent::RedrawRequested) {
            self.inner.surface_event(cx, surface, event);
            return;
        }
        self.delivered += 1;
        let declines = self.delivered > 1 && self.delivered % 2 == 0;
        if declines {
            if let Some(surface) = cx.surface(surface) {
                surface.redraw_declined();
                DECLINED.fetch_add(1, Ordering::Relaxed);
                surface.request_redraw();
            }
            return;
        }
        self.inner.surface_event(cx, surface, event);
        if support::FRAMES.fetch_add(1, Ordering::Relaxed) + 1 >= FRAMES {
            cx.request_exit();
        } else if let Some(surface) = cx.surface(surface) {
            surface.request_redraw();
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        self.inner.idle(cx)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.inner.deadline_reached(cx);
    }

    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        self.inner.shutting_down(cx);
    }
}

fn main() {
    const PROPERTY: &str = "a declined redraw commits nothing and delays nothing";

    support::tracing();
    support::watchdog(PROPERTY);

    let handler = DeclineEveryOther {
        inner: match support::animated_application("declined redraw") {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        },
        delivered: 0,
    };

    let mut app = match WaylandApp::new(handler) {
        Ok(app) => app,
        Err(PlatformError::Backend(reason)) => {
            support::skipped(PROPERTY, &format!("no compositor to run on: {reason}"));
            return;
        }
        Err(other) => panic!("the compositor refused the application: {other}"),
    };

    let started = Instant::now();
    if let Err(error) = app.run() {
        panic!("the loop stopped: {error}");
    }
    let elapsed = started.elapsed();

    if support::NO_DEVICE.load(Ordering::Relaxed) {
        support::skipped(
            PROPERTY,
            "this machine has no graphics device to draw through",
        );
        return;
    }

    let frames = support::FRAMES.load(Ordering::Relaxed);
    let declined = DECLINED.load(Ordering::Relaxed);
    assert!(
        frames >= FRAMES,
        "the loop finished after {frames} frames of {FRAMES}: a decline stranded the chain"
    );
    assert!(
        declined >= FRAMES - 1,
        "{declined} redraws were declined across {frames} drawn frames, so the declines never \
         interleaved and the property was not exercised"
    );
    // The failure this wall bound catches is the expensive one: on a compositor that ignores
    // empty commits, a declined redraw that wrongly committed waits out the watchdog's whole
    // 200ms grace, so seven declines add over 1.4s on top of the drawing. The cheap form — one
    // answered callback per decline on a compositor that answers them — hides inside the
    // animation's own cadence here and is pinned by the pacer's unit tests instead. Healthy runs
    // measure 300-750ms on this document.
    assert!(
        elapsed.as_millis() < 1_500,
        "{frames} frames with {declined} declines took {elapsed:?}: the declines are waiting \
         out graces"
    );

    let turns = app.turns();
    assert!(
        turns < (frames + declined) * 200,
        "{turns} turns for {frames} frames: the loop is spinning rather than waiting"
    );

    support::passed(
        PROPERTY,
        &format!(
            "{frames} frames with {declined} declines interleaved, in {elapsed:?}, {turns} turns"
        ),
    );
}
