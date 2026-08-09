//! The platform itself: a clock, a clipboard, a waker and its offscreen surfaces.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use zgui_geom::{DevicePx, Size};
use zgui_platform::{
    Clipboard, Clock, ColorScheme, MonitorInfo, PlatformCapabilities, PlatformCx, PlatformError,
    ScrollSettings, Surface, SurfaceAttributes, SurfaceId, VirtualClock, WakeReason, Waker,
};

use crate::clipboard::MemoryClipboard;
use crate::surface::OffscreenSurface;
use crate::waker::RecordingWaker;

/// The extent of a surface created without one, in physical pixels.
const DEFAULT_SIZE: Size<DevicePx, zgui_geom::Device> = Size::new(DevicePx(640.0), DevicePx(480.0));

/// A platform with no windowing system behind it.
///
/// Held by a [`Harness`](crate::Harness), which is what drives an application against it. It is
/// also usable on its own by anything that wants the contract's answers without a loop.
pub struct Headless {
    /// The clock, which only moves when it is told to.
    clock: Arc<VirtualClock>,
    /// The clipboards, which are two values in memory.
    clipboard: MemoryClipboard,
    /// What this platform declares it can do.
    capabilities: PlatformCapabilities,
    /// Where a wake from another thread is queued.
    waker: Arc<RecordingWaker>,
    /// The surfaces that exist.
    surfaces: Mutex<Vec<Arc<OffscreenSurface>>>,
    /// The next surface number, never reused.
    next_surface: AtomicU64,
    /// The outputs this platform reports.
    monitors: Vec<MonitorInfo>,
    /// The desktop's light or dark preference, when one has been declared.
    color_scheme: Mutex<Option<ColorScheme>>,
    /// What this platform says a scroll from its devices means.
    scroll: Mutex<ScrollSettings>,
    /// Whether the loop has been asked to finish.
    exiting: AtomicBool,
}

impl Default for Headless {
    fn default() -> Self {
        Self::new()
    }
}

impl Headless {
    /// A platform with no surfaces, a clock at its origin and empty clipboards.
    pub fn new() -> Self {
        let waker = Arc::new(RecordingWaker::default());
        let clipboard = MemoryClipboard::default();
        // A read started through the loop is answered through the loop, which needs the waker the
        // platform hands out.
        clipboard.attach_waker(Arc::clone(&waker) as Arc<dyn Waker>);
        Self {
            clock: Arc::new(VirtualClock::new()),
            clipboard,
            capabilities: PlatformCapabilities::none(),
            waker,
            surfaces: Mutex::new(Vec::new()),
            next_surface: AtomicU64::new(1),
            monitors: Vec::new(),
            color_scheme: Mutex::new(None),
            scroll: Mutex::new(ScrollSettings::desktop()),
            exiting: AtomicBool::new(false),
        }
    }

    /// The same platform declaring `capabilities`.
    ///
    /// Nothing here can actually do any of them. It exists so that a component whose behaviour
    /// depends on what the desktop offers can be exercised under each answer, which is the only
    /// way to find out that it degrades rather than disappearing.
    pub fn with_capabilities(mut self, capabilities: PlatformCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// The same platform reporting `monitors`.
    pub fn with_monitors(mut self, monitors: Vec<MonitorInfo>) -> Self {
        self.monitors = monitors;
        self
    }

    /// The same platform declaring what a scroll from its devices means.
    ///
    /// The headless backend has no devices and no desktop, so every answer here is a *stated* one —
    /// which is exactly what makes the scrolling contract testable. A wheel that arrives inverted,
    /// a desktop that means five lines by one detent and a platform that animates its own detents
    /// are three different behaviours above this seam, and without a backend that can say each of
    /// them in turn none of the three is reachable from a test at all.
    pub fn with_scroll_settings(self, settings: ScrollSettings) -> Self {
        self.set_scroll_settings(settings);
        self
    }

    /// Declares what a scroll from this platform's devices means, after it has been built.
    pub fn set_scroll_settings(&self, settings: ScrollSettings) {
        *self.scroll.lock().expect("the settings are not poisoned") = settings;
    }

    /// Declares the desktop's light or dark preference.
    pub fn set_color_scheme(&self, scheme: Option<ColorScheme>) {
        *self
            .color_scheme
            .lock()
            .expect("the preference is not poisoned") = scheme;
    }

    /// The clock, as itself rather than as the contract's view of it.
    pub fn virtual_clock(&self) -> &VirtualClock {
        &self.clock
    }

    /// The offscreen surface with this identifier, as itself.
    ///
    /// A test asserts against what a surface recorded, and the contract deliberately offers no way
    /// to ask.
    pub fn offscreen(&self, id: SurfaceId) -> Option<Arc<OffscreenSurface>> {
        self.surfaces
            .lock()
            .expect("the surface list is not poisoned")
            .iter()
            .find(|surface| surface.id() == id)
            .map(Arc::clone)
    }

    /// Every offscreen surface that exists, as itself.
    pub fn offscreens(&self) -> Vec<Arc<OffscreenSurface>> {
        self.surfaces
            .lock()
            .expect("the surface list is not poisoned")
            .iter()
            .map(Arc::clone)
            .collect()
    }

    /// Takes every wake delivered since the last call.
    pub fn drain_wakes(&self) -> Vec<WakeReason> {
        self.waker.drain()
    }

    /// Whether anything is waiting to be delivered as a wake.
    pub fn has_pending_wakes(&self) -> bool {
        self.waker.pending() > 0
    }
}

impl PlatformCx for Headless {
    fn create_surface(
        &self,
        attributes: &SurfaceAttributes,
    ) -> Result<Arc<dyn Surface>, PlatformError> {
        let size = attributes.size.map_or(DEFAULT_SIZE, |size| {
            Size::new(DevicePx(size.width.0), DevicePx(size.height.0))
        });
        let id = SurfaceId::new(self.next_surface.fetch_add(1, Ordering::Relaxed));
        let surface = Arc::new(OffscreenSurface::new(id, size));
        surface.set_title(attributes.title.as_str());
        surface.set_requested_attributes(attributes);
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
            .retain(|surface| surface.id() != id);
    }

    fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.offscreen(id)
            .map(|surface| surface as Arc<dyn Surface>)
    }

    fn surfaces(&self) -> Vec<Arc<dyn Surface>> {
        self.offscreens()
            .into_iter()
            .map(|surface| surface as Arc<dyn Surface>)
            .collect()
    }

    fn monitors(&self) -> Vec<MonitorInfo> {
        self.monitors.clone()
    }

    fn primary_monitor(&self) -> Option<MonitorInfo> {
        self.monitors.first().cloned()
    }

    fn color_scheme(&self) -> Option<ColorScheme> {
        *self
            .color_scheme
            .lock()
            .expect("the preference is not poisoned")
    }

    fn clipboard(&self) -> &dyn Clipboard {
        &self.clipboard
    }

    fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    fn scroll_settings(&self) -> ScrollSettings {
        *self.scroll.lock().expect("the settings are not poisoned")
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

#[cfg(test)]
mod tests {
    use super::Headless;
    use zgui_geom::{CssPx, Size};
    use zgui_platform::{PlatformCx, SurfaceAttributes};

    #[test]
    fn a_surface_is_created_hidden_with_the_extent_it_asked_for() {
        let platform = Headless::new();
        let surface = platform
            .create_surface(
                &SurfaceAttributes::new("headless")
                    .with_size(Size::new(CssPx(800.0), CssPx(600.0))),
            )
            .expect("always creatable");
        assert_eq!(surface.size().width.0, 800.0);

        let offscreen = platform.offscreen(surface.id()).expect("the same surface");
        assert!(
            !offscreen.is_visible(),
            "a surface is shown by its first frame, not by creation"
        );
        assert_eq!(offscreen.title(), "headless");
    }

    #[test]
    fn every_surface_gets_its_own_identity() {
        let platform = Headless::new();
        let first = platform
            .create_surface(&SurfaceAttributes::new("a"))
            .expect("creatable");
        let second = platform
            .create_surface(&SurfaceAttributes::new("b"))
            .expect("creatable");
        assert_ne!(first.id(), second.id());
        assert_eq!(platform.surfaces().len(), 2);
    }
}
