//! The headless backend, written out in full and driven through the contract.

mod app;
mod clipboard;
pub(in crate::backends) mod surface;
mod waker;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zgui_geom::{DevicePx, Size};

use crate::backends::headless::clipboard::MemoryClipboard;
use crate::backends::headless::surface::OffscreenSurface;
use crate::backends::headless::waker::RecordingWaker;
use crate::capabilities::PlatformCapabilities;
use crate::clipboard::Clipboard;
use crate::clock::{Clock, VirtualClock};
use crate::cx::PlatformCx;
use crate::error::PlatformError;
use crate::monitor::MonitorInfo;
use crate::surface::{Surface, SurfaceAttributes, SurfaceId};
use crate::theme::ColorScheme;
use crate::waker::Waker;

/// The headless platform: a clock, a clipboard, a waker and its offscreen surfaces.
pub(super) struct Headless {
    clock: Arc<VirtualClock>,
    clipboard: MemoryClipboard,
    capabilities: PlatformCapabilities,
    waker: Arc<RecordingWaker>,
    surfaces: Mutex<Vec<Arc<OffscreenSurface>>>,
    next_surface: AtomicU64,
    exiting: AtomicBool,
}

impl Headless {
    /// A platform with no surfaces, a clock at its origin and an empty clipboard.
    pub(super) fn new() -> Self {
        let waker = Arc::new(RecordingWaker::default());
        let clipboard = MemoryClipboard::default();
        // A read started through the loop is answered through the loop.
        clipboard.attach_waker(Arc::clone(&waker) as Arc<dyn crate::waker::Waker>);
        Self {
            clock: Arc::new(VirtualClock::new()),
            clipboard,
            capabilities: PlatformCapabilities::none(),
            waker,
            surfaces: Mutex::new(Vec::new()),
            next_surface: AtomicU64::new(1),
            exiting: AtomicBool::new(false),
        }
    }

    /// Moves the clock, and reports whether that crossed `deadline`.
    ///
    /// Crossing a deadline is the edge the real loop has and a naive test double does not: the
    /// deadline itself has to produce the redraw request, or a test cannot tell "the deadline woke
    /// us" from "the test asked for a frame".
    pub(super) fn advance(&self, by: Duration, deadline: Option<Instant>) -> bool {
        self.clock.advance(by);
        deadline.is_some_and(|deadline| self.clock.now() >= deadline)
    }

    /// The offscreen surface with this identifier, as itself rather than as the trait.
    ///
    /// A test asserts against what a surface recorded, and the trait deliberately offers no way
    /// to ask.
    pub(super) fn offscreen(&self, id: SurfaceId) -> Option<Arc<OffscreenSurface>> {
        self.surfaces
            .lock()
            .expect("the surface list is not poisoned")
            .iter()
            .find(|surface| surface.id() == id)
            .map(Arc::clone)
    }

    /// The queue of wakes delivered so far, emptied by the read.
    pub(super) fn drain_wakes(&self) -> Vec<crate::app::WakeReason> {
        self.waker.drain()
    }
}

impl PlatformCx for Headless {
    fn create_surface(
        &self,
        attributes: &SurfaceAttributes,
    ) -> Result<Arc<dyn Surface>, PlatformError> {
        let size = attributes
            .size
            .map_or(Size::new(DevicePx(640.0), DevicePx(480.0)), |size| {
                Size::new(DevicePx(size.width.0), DevicePx(size.height.0))
            });
        let id = SurfaceId::new(self.next_surface.fetch_add(1, Ordering::Relaxed));
        let surface = Arc::new(OffscreenSurface::new(id, size));
        self.surfaces
            .lock()
            .expect("the surface list is not poisoned")
            .push(Arc::clone(&surface));
        Ok(surface as Arc<dyn Surface>)
    }

    fn destroy_surface(&self, id: SurfaceId) {
        self.surfaces
            .lock()
            .expect("the surface list is not poisoned")
            .retain(|surface| Surface::id(surface.as_ref()) != id);
    }

    fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.offscreen(id)
            .map(|surface| surface as Arc<dyn Surface>)
    }

    fn surfaces(&self) -> Vec<Arc<dyn Surface>> {
        self.surfaces
            .lock()
            .expect("the surface list is not poisoned")
            .iter()
            .map(|surface| Arc::clone(surface) as Arc<dyn Surface>)
            .collect()
    }

    fn monitors(&self) -> Vec<MonitorInfo> {
        Vec::new()
    }

    fn primary_monitor(&self) -> Option<MonitorInfo> {
        None
    }

    fn color_scheme(&self) -> Option<ColorScheme> {
        None
    }

    fn clipboard(&self) -> &dyn Clipboard {
        &self.clipboard
    }

    fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.clock) as Arc<dyn Clock>
    }

    fn waker(&self) -> Arc<dyn Waker> {
        Arc::clone(&self.waker) as Arc<dyn Waker>
    }

    fn request_exit(&self) {
        self.exiting.store(true, Ordering::Relaxed);
    }

    fn is_exiting(&self) -> bool {
        self.exiting.load(Ordering::Relaxed)
    }
}
