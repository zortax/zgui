//! The application's side of the contract: what the platform calls, and when.

mod idle;
mod wake;

pub use crate::app::idle::IdlePolicy;
pub use crate::app::wake::WakeReason;

use crate::cx::PlatformCx;
use crate::surface::{SurfaceEvent, SurfaceId};

/// What a platform backend calls, in the order it calls it.
///
/// This is the whole of the inward contract. A backend drives it; the framework implements it; and
/// because every method is handed its context rather than reaching for one, nothing above this
/// trait ever holds a platform object across a callback — which is what makes the context safe to
/// be borrowed, single-threaded and short-lived on every platform that has one.
///
/// # The order
///
/// [`AppHandler::surfaces_available`] comes first and may come again. Surfaces cannot be created
/// before it and must not outlive [`AppHandler::surfaces_lost`]. On a desktop the second is
/// usually never called at all; on a platform that suspends applications it is called every time
/// the application goes to the background, and a surface still held at that point is a crash.
///
/// [`AppHandler::surface_event`] carries everything that happened to a surface, including the
/// request to draw. [`AppHandler::wake`] carries everything that happened elsewhere.
/// [`AppHandler::idle`] is asked, once per turn, how the loop should park.
pub trait AppHandler: 'static {
    /// Surfaces may now be created.
    ///
    /// Called at least once, before any other method that concerns a surface. It may be called
    /// again after [`AppHandler::surfaces_lost`].
    fn surfaces_available(&mut self, cx: &dyn PlatformCx);

    /// Every surface is now invalid and must be dropped before this returns.
    ///
    /// Anything built on a surface — a swap chain, an accessibility adapter — goes with it. The
    /// default does nothing, which is correct on a platform where this never happens.
    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        let _ = cx;
    }

    /// Something happened to one surface.
    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent);

    /// Something happened that was not about a surface.
    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason);

    /// How the loop should park, asked once per turn before it blocks.
    ///
    /// This runs on every pointer motion, so it has to be cheap. The default parks until the
    /// platform has something to say, which is what an idle interface should do.
    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        let _ = cx;
        IdlePolicy::Block
    }

    /// A deadline asked for earlier has been reached.
    ///
    /// Reaching a deadline does not draw anything by itself. The loop wakes, this is called, and
    /// whatever was waiting on the deadline asks the surfaces it concerns to redraw. Without that
    /// step a timer never fires and an animation never advances on an otherwise idle loop, and the
    /// symptom is not a stall but a loop that spins while running no frames at all.
    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        let _ = cx;
    }

    /// The loop is finishing.
    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        let _ = cx;
    }
}

impl<T: AppHandler + ?Sized> AppHandler for Box<T> {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        (**self).surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        (**self).surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        (**self).surface_event(cx, surface, event);
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        (**self).wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        (**self).idle(cx)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        (**self).deadline_reached(cx);
    }

    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        (**self).shutting_down(cx);
    }
}
