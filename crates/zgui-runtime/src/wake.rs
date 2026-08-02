//! The wake edge: how anything that is not an input event asks for a frame.
//!
//! A parked event loop is blocked on the windowing system, not on the reactive layer. So a signal
//! written on a worker thread, a future resolving, an image finishing its decode and a background
//! build completing all mark work ready and then wait for the user to move the mouse — unless
//! something pings the loop from where the work happened. That ping is this module.
//!
//! It has two halves, and the second is what keeps the first from being a stampede. A wake raised
//! from *outside* a frame goes to the platform, which asks the surfaces it belongs to for a frame.
//! A wake raised from *inside* one is folded into "another frame is owed" instead: an effect that
//! writes a signal would otherwise ask the loop for a frame from inside the frame that is already
//! running, once per write.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zgui_platform::{SurfaceId, WakeReason, Waker};
use zgui_reactive::FrameWaker;

/// Whether a frame is in flight, and whether one is owed when it ends.
///
/// Shared with every waker built over it, and read from arbitrary threads: a wake arrives from
/// wherever the work finished, which is precisely why this cannot be thread-local state.
#[derive(Debug, Default)]
pub struct FrameGate {
    /// Whether a frame is between its first phase and its last.
    in_frame: AtomicBool,
    /// Whether anything asked for a frame while one was in flight.
    another_frame: AtomicBool,
}

impl FrameGate {
    /// A gate with no frame in flight and nothing owed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a frame is in flight.
    pub fn in_frame(&self) -> bool {
        self.in_frame.load(Ordering::Relaxed)
    }

    /// Whether another frame is owed.
    pub fn needs_another_frame(&self) -> bool {
        self.another_frame.load(Ordering::Relaxed)
    }

    /// Records that a request arrived, and reports whether it was deferred rather than forwarded.
    ///
    /// Every in-frame requester routes through here — a mutation closing its batch, a timer's
    /// callback pinging the executor, an observation delivery writing a slot, and the reactive
    /// flush itself — so that the frame's last phase converts all of them into exactly one request
    /// rather than each of them producing one.
    pub fn request(&self) -> bool {
        if self.in_frame() {
            self.another_frame.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Records that the stage which services what has been asked for is about to run.
    ///
    /// A frame flushes the reactive graph part-way through itself, so a wake raised before that
    /// point — by a handler running in this frame's own dispatch, by a timer's callback, by a task
    /// that finished while this frame was starting — is answered by this frame and owes nothing.
    /// Without this every interaction bought a second frame that damaged nothing and presented a
    /// surface identical to the one just presented.
    ///
    /// It is called immediately before the flush, and that is what makes it safe against a wake
    /// raised on another thread: a wake is sent after the work it announces has been published, so
    /// a wake this clears was published before the flush that follows and is therefore serviced by
    /// it. Called *after* the flush, the same line would lose the work of anything that arrived in
    /// between.
    pub fn requests_serviced(&self) {
        self.another_frame.store(false, Ordering::Relaxed);
    }

    /// Marks the start of a frame, clearing what the last one owed.
    pub fn begin_frame(&self) {
        self.in_frame.store(true, Ordering::Relaxed);
        self.another_frame.store(false, Ordering::Relaxed);
    }

    /// Marks the end of a frame, and reports whether anything asked for another during it.
    pub fn end_frame(&self) -> bool {
        self.in_frame.store(false, Ordering::Relaxed);
        self.another_frame.swap(false, Ordering::Relaxed)
    }
}

/// The reactive layer's wake, routed to the platform.
///
/// The surfaces are named rather than left implicit, because an image decoding for one window is
/// not a reason to redraw another.
pub struct RuntimeWaker {
    /// Where a wake goes when no frame is running.
    platform: Arc<dyn Waker>,
    /// Whether a frame is running, and what it owes.
    gate: Arc<FrameGate>,
    /// The surfaces this wake concerns.
    surfaces: Mutex<Vec<SurfaceId>>,
}

impl RuntimeWaker {
    /// A waker that pings `platform` for the surfaces registered on it.
    pub fn new(platform: Arc<dyn Waker>, gate: Arc<FrameGate>) -> Self {
        Self {
            platform,
            gate,
            surfaces: Mutex::new(Vec::new()),
        }
    }

    /// Whether a frame is running, and what it owes.
    pub fn gate(&self) -> &Arc<FrameGate> {
        &self.gate
    }

    /// Adds a surface to the set a wake concerns.
    pub fn owns(&self, surface: SurfaceId) {
        let mut surfaces = self.surfaces.lock().expect("the set is not poisoned");
        if !surfaces.contains(&surface) {
            surfaces.push(surface);
        }
    }

    /// Removes a surface from that set, which a closed window does.
    pub fn disowns(&self, surface: SurfaceId) {
        self.surfaces
            .lock()
            .expect("the set is not poisoned")
            .retain(|held| *held != surface);
    }
}

impl FrameWaker for RuntimeWaker {
    fn wake(&self) {
        if self.gate.request() {
            return;
        }
        let surfaces = self
            .surfaces
            .lock()
            .expect("the set is not poisoned")
            .clone();
        if surfaces.is_empty() {
            return;
        }
        self.platform.wake(WakeReason::ReactiveWork {
            surfaces: surfaces.into_boxed_slice(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameGate, RuntimeWaker};
    use std::sync::{Arc, Mutex};
    use zgui_platform::{SurfaceId, WakeReason, Waker};
    use zgui_reactive::FrameWaker;

    /// A platform waker that records what it was handed.
    #[derive(Default)]
    struct Recording(Mutex<Vec<Vec<SurfaceId>>>);

    impl Waker for Recording {
        fn wake(&self, reason: WakeReason) {
            self.0
                .lock()
                .expect("not poisoned")
                .push(reason.surfaces().to_vec());
        }
    }

    #[test]
    fn a_wake_outside_a_frame_reaches_the_platform_naming_its_own_surfaces() {
        let platform = Arc::new(Recording::default());
        let gate = Arc::new(FrameGate::new());
        let waker = RuntimeWaker::new(Arc::clone(&platform) as Arc<dyn Waker>, gate);
        waker.owns(SurfaceId::new(7));

        waker.wake();

        let recorded = platform.0.lock().expect("not poisoned");
        assert_eq!(recorded.as_slice(), [vec![SurfaceId::new(7)]]);
    }

    #[test]
    fn a_wake_inside_a_frame_is_owed_rather_than_sent() {
        let platform = Arc::new(Recording::default());
        let gate = Arc::new(FrameGate::new());
        let waker = RuntimeWaker::new(Arc::clone(&platform) as Arc<dyn Waker>, Arc::clone(&gate));
        waker.owns(SurfaceId::new(1));

        gate.begin_frame();
        for _ in 0..1_000 {
            waker.wake();
        }
        assert!(
            platform.0.lock().expect("not poisoned").is_empty(),
            "a thousand in-frame wakes must not become a thousand redraw requests"
        );
        assert!(gate.end_frame(), "and the frame still owes exactly one");
        assert!(!gate.needs_another_frame(), "which is taken by reading it");
    }

    #[test]
    fn a_wake_belonging_to_no_surface_goes_nowhere() {
        let platform = Arc::new(Recording::default());
        let gate = Arc::new(FrameGate::new());
        let waker = RuntimeWaker::new(Arc::clone(&platform) as Arc<dyn Waker>, gate);
        waker.owns(SurfaceId::new(3));
        waker.disowns(SurfaceId::new(3));
        waker.wake();
        assert!(platform.0.lock().expect("not poisoned").is_empty());
    }
}
