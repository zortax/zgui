//! The application handler, wrapped so that every event's arrival is timestamped.
//!
//! This is the outermost point inside the process at which an input event can be observed: the
//! windowing backend has decoded it and is about to hand it to the framework. Everything the
//! framework then does — dispatch, restyle, layout, paint, draw, present — happens between this
//! mark and the renderer's, so the two together bound the whole of what this program controls.

use std::time::Instant;

use zgui_platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};

use crate::tape::Shared;

/// A handler that records when each callback began, and delegates.
pub(crate) struct Timed {
    /// The handler that does the work.
    inner: Box<dyn AppHandler>,
    /// Where the moments go.
    tape: Shared,
}

impl Timed {
    /// Wraps `inner`, recording into `tape`.
    pub(crate) fn new(inner: Box<dyn AppHandler>, tape: Shared) -> Self {
        Self { inner, tape }
    }
}

impl AppHandler for Timed {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.tape.borrow_mut().now("avail", "");
        self.inner.surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        let at = Instant::now();
        let (stage, detail) = describe(&event);
        self.tape.borrow_mut().at(at, stage, detail);
        self.inner.surface_event(cx, surface, event);
        self.tape.borrow_mut().now("ev.end", stage);
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.tape.borrow_mut().now("wake", "");
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        let policy = self.inner.idle(cx);
        let mut tape = self.tape.borrow_mut();
        tape.now(
            "park",
            match policy {
                IdlePolicy::Block => "block",
                IdlePolicy::BlockUntil(_) => "until",
                IdlePolicy::Spin => "spin",
                _ => "other",
            },
        );
        tape.flush_if_due();
        policy
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.tape.borrow_mut().now("deadline", "");
        self.inner.deadline_reached(cx);
    }

    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        self.inner.shutting_down(cx);
    }
}

/// The stage an event is recorded under, and what distinguishes it from its neighbours.
///
/// Input events get their own stages because they are the ones a latency is measured from; the
/// rest share one, because what matters about them is only that a turn happened.
fn describe(event: &SurfaceEvent) -> (&'static str, String) {
    match event {
        SurfaceEvent::Pointer { action, event, .. } => (
            "in.pointer",
            format!(
                "{action:?} {:.1},{:.1}",
                event.position.x.0, event.position.y.0
            ),
        ),
        SurfaceEvent::Key { state, .. } => ("in.key", format!("{state:?}")),
        SurfaceEvent::Wheel { .. } => ("in.wheel", String::new()),
        SurfaceEvent::Resized(size) => ("in.resize", format!("{}x{}", size.width.0, size.height.0)),
        SurfaceEvent::ScaleFactorChanged { scale_factor, size } => (
            "in.scale",
            format!("{scale_factor} {}x{}", size.width.0, size.height.0),
        ),
        SurfaceEvent::RedrawRequested => ("ev.redraw", String::new()),
        other => ("ev.other", format!("{:.20}", format!("{other:?}"))),
    }
}
