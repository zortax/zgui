//! A surface reports the refresh rate of the output it is on.
//!
//! The runtime paces animations against this rate, and paces at sixty hertz when it has none. The
//! presentation feedback states the interval on most outputs. A headless output and a
//! variable-refresh output state none, and the rate must then come from the mode of the output the
//! surface entered. Without it, a 240 Hz headless output shows an animation at 60 frames a second.
//!
//! The test discriminates only on an output whose presentation feedback states no refresh, such as
//! a nested headless sway with `output HEADLESS-1 mode --custom 1280x720@240Hz`. On a compositor
//! that states the refresh, it passes with or without the output rate. The surface's rate must
//! match the rate of one of the outputs the compositor advertises.

#[path = "support/mod.rs"]
mod support;

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use zgui_platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};

/// How long the application runs before the loop finishes.
///
/// Measured in time, because a surface enters an output only after the compositor has shown a
/// buffer, and many redraws can run before that happens.
const RUN: Duration = Duration::from_millis(1_500);

/// The rate the surface reported at its last frame, the rates the outputs state, and whether a
/// frame reached the screen.
static SEEN: Mutex<(Option<u32>, Vec<u32>, bool)> = Mutex::new((None, Vec::new(), false));

/// An application that records the surface's rate at every frame.
struct Recording<A: AppHandler> {
    /// The application.
    inner: A,
    /// When the first frame was delivered.
    started: Option<Instant>,
}

impl<A: AppHandler> AppHandler for Recording<A> {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        if matches!(event, SurfaceEvent::RedrawRequested)
            && let Some(held) = cx.surface(surface)
            && let Ok(mut seen) = SEEN.lock()
        {
            seen.0 = held.refresh_rate_millihertz();
            seen.1 = cx
                .monitors()
                .iter()
                .filter_map(|monitor| monitor.refresh_rate_millihertz)
                .collect();
            seen.2 |= held
                .presentation_timing()
                .is_some_and(|timing| timing.last_presented.is_some());
            let started = *self.started.get_or_insert_with(Instant::now);
            if started.elapsed() >= RUN {
                cx.request_exit();
            }
        }
        self.inner.surface_event(cx, surface, event);
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
    const PROPERTY: &str = "a surface reports the refresh rate of the output it is on";

    support::tracing();
    support::watchdog(PROPERTY);

    let runtime = match support::animated_application("output rate") {
        Ok(runtime) => runtime,
        Err(error) => {
            support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
            return;
        }
    };
    let handler = Recording {
        inner: support::StopAfter {
            inner: runtime,
            frames: u64::MAX,
            sustained: true,
        },
        started: None,
    };
    let Some(mut app) = support::loop_for(PROPERTY, handler) else {
        return;
    };
    if let Err(error) = app.run() {
        panic!("the loop stopped: {error}");
    }
    if support::NO_DEVICE.load(Ordering::Relaxed) {
        support::skipped(
            PROPERTY,
            "this machine has no graphics device to draw through",
        );
        return;
    }

    let (reported, outputs, presented) = SEEN.lock().map(|seen| seen.clone()).unwrap_or_default();
    if outputs.is_empty() {
        support::skipped(PROPERTY, "no output states a refresh rate");
        return;
    }
    if !presented {
        support::skipped(PROPERTY, "no frame reached the screen");
        return;
    }
    let reported = reported.unwrap_or_else(|| {
        panic!("the surface reported no refresh rate; the outputs state {outputs:?} mHz")
    });
    // Within a tenth of a percent: the feedback states the interval in nanoseconds, and a mode
    // states the rate in thousandths of a hertz.
    let matches = outputs
        .iter()
        .any(|&rate| reported.abs_diff(rate) <= rate / 1_000);
    assert!(
        matches,
        "the surface reported {reported} mHz; the outputs state {outputs:?} mHz"
    );
    support::passed(PROPERTY, &format!("{reported} mHz"));
}
