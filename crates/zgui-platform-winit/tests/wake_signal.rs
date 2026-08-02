//! A signal written from outside any input event still produces a frame.
//!
//! This is the wake path's first property and the one whose absence is hardest to notice: nothing
//! crashes, nothing logs, and the interface simply stops reflecting anything that happens off the
//! main thread. A value written on a worker thread marks something dirty, and the loop is blocked
//! on the compositor's socket where no amount of dirtiness reaches it. The write has to interrupt
//! the block, and the interruption has to end in a frame.
//!
//! The frame is counted **relative to the wake** rather than absolutely, because the platform asks
//! for a first paint of its own when a window is created. Counting from zero would let that first
//! paint stand in for the frame the wake was supposed to produce, which is exactly the vacuous pass
//! this property exists to rule out.

#[path = "support/mod.rs"]
mod support;

use std::time::Duration;

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
};
use zgui_platform_winit::WinitApp;

/// An application that draws nothing on its own account.
#[derive(Default)]
struct Probe {
    /// How many wakes arrived.
    wakes: u32,
    /// How many frames ran in total, the platform's own first paint included.
    frames: u32,
    /// How many had run at the moment the wake was delivered.
    frames_at_wake: Option<u32>,
}

impl AppHandler for Probe {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        let surface = cx
            .create_surface(&SurfaceAttributes::new("signal"))
            .expect("a window could not be created");
        let id = surface.id();

        // Taken here, exactly as a view takes it while it is being built, and used from a thread
        // that knows nothing about the loop it is reaching.
        support::wake_after(cx.waker(), Duration::from_millis(120), move || {
            WakeReason::ReactiveWork {
                surfaces: Box::from([id]),
            }
        });
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
        if !matches!(event, SurfaceEvent::RedrawRequested) {
            return;
        }
        self.frames += 1;
        if self.wakes > 0 {
            cx.request_exit();
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.wakes += 1;
        self.frames_at_wake = Some(self.frames);
        // The work belongs to the surfaces it names and to no others: an image decoding for one
        // window is not a reason to redraw another.
        for id in reason.surfaces() {
            if let Some(surface) = cx.surface(*id) {
                surface.request_redraw();
            }
        }
    }

    fn idle(&mut self, _cx: &dyn PlatformCx) -> IdlePolicy {
        IdlePolicy::Block
    }
}

fn main() {
    const PROPERTY: &str = "a signal written from outside any input event still produces a frame";

    let Some(event_loop) = support::event_loop() else {
        return;
    };
    support::watchdog(PROPERTY);

    let mut app = WinitApp::new(&event_loop, Probe::default());
    event_loop.run_app(&mut app).expect("the loop ran");

    let probe = app.handler();
    assert_eq!(
        probe.wakes, 1,
        "the write on the other thread never reached the loop"
    );
    let at_wake = probe
        .frames_at_wake
        .expect("the wake was delivered, so this was recorded with it");
    assert_eq!(
        probe.frames,
        at_wake + 1,
        "the wake reached the loop and produced no frame of its own, which is the stall this \
         asserts on"
    );
    assert_eq!(
        app.park().resumes(),
        0,
        "no deadline was involved, so none may be reported"
    );
    // The wake, the frame it asked for, and whatever the platform had to say about a new window: a
    // handful of turns, not a loop that never slept.
    assert!(
        app.turns() < 64,
        "the loop took {} turns to service one wake",
        app.turns()
    );

    support::passed(PROPERTY);
}
