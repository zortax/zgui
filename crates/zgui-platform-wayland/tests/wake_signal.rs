//! A signal written from outside every event stream still produces a frame.
//!
//! Work finishing on a worker thread is one of the four things that can want a frame, and it is
//! the one nothing in the loop can notice: the loop is asleep on the compositor's socket, and a
//! value written on another thread makes no sound there. So the write ends at a waker, the waker
//! interrupts the sleep, and the reason is delivered on the loop's own thread — where asking for a
//! frame is safe.
//!
//! Asserted here rather than reasoned about, because every part of that path is a place the frame
//! can be lost: a signal that never wakes the loop, a wake that arrives with no reason, a reason
//! that reaches no surface.

#[path = "support/mod.rs"]
mod support;

use std::time::Duration;

use zgui_platform::{
    AppHandler, PlatformCx, Surface, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
};

/// How long the loop is left asleep before the signal arrives.
const ASLEEP: Duration = Duration::from_millis(400);

/// An application that draws once, sleeps, and is woken from elsewhere.
#[derive(Default)]
struct Probe {
    /// The window, once it exists.
    surface: Option<SurfaceId>,
    /// How many frames have run.
    frames: u32,
    /// How many frames had run when the signal arrived.
    frames_at_signal: Option<u32>,
    /// Whether the wake carried the surface it belonged to.
    named_the_surface: bool,
}

impl AppHandler for Probe {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        let surface = cx
            .create_surface(&SurfaceAttributes::new("signal"))
            .expect("a window could not be created");
        self.surface = Some(Surface::id(surface.as_ref()));
        let id = Surface::id(surface.as_ref());
        support::wake_after(cx.waker(), ASLEEP, move || WakeReason::ReactiveWork {
            surfaces: Box::from([id]),
        });
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
        if !matches!(event, SurfaceEvent::RedrawRequested) {
            return;
        }
        self.frames += 1;
        // The frame the signal asked for is the second one; the first is the window opening.
        if self.frames_at_signal.is_some() {
            cx.request_exit();
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.frames_at_signal = Some(self.frames);
        self.named_the_surface = reason.surfaces() == [self.surface.expect("the window exists")];
        // A wake draws nothing by itself. Turning it into a request is the edge being asserted.
        for surface in reason.surfaces() {
            if let Some(surface) = cx.surface(*surface) {
                surface.request_redraw();
            }
        }
    }
}

fn main() {
    const PROPERTY: &str = "a signal written from outside any input event still produces a frame";

    support::watchdog(PROPERTY);
    let Some(mut app) = support::loop_for(PROPERTY, Probe::default()) else {
        return;
    };
    app.run().expect("the loop ran");

    let probe = app.handler();
    let at_signal = probe
        .frames_at_signal
        .expect("the loop was never woken from the other thread at all");
    assert!(
        probe.named_the_surface,
        "the wake arrived without the surface the work belonged to"
    );
    assert!(
        probe.frames > at_signal,
        "the loop woke and drew nothing: the wake never became a request"
    );

    support::passed(
        PROPERTY,
        &format!(
            "{} frames, {at_signal} of them before the signal",
            probe.frames
        ),
    );
}
