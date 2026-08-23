//! The borrowed platform context, valid only inside a callback.

use std::sync::Arc;

use zgui_platform::{
    Clipboard, Clock, ColorScheme, MonitorInfo, PlatformCapabilities, PlatformCx, PlatformError,
    Surface, SurfaceAttributes, SurfaceId, Waker,
};

use crate::driver::WaylandState;

/// Everything the compositor offers, for the duration of one callback.
///
/// Borrowed and never stored, exactly as the contract describes: this holds the loop's own state,
/// which is being borrowed for the length of one delivery and is mutable again the moment it
/// returns. Nothing here outlives the callback except the three things that say so in their own
/// types — a surface, a waker and a clock.
pub struct WaylandCx<'a> {
    /// The loop's state.
    state: &'a WaylandState,
}

impl<'a> WaylandCx<'a> {
    /// A context over `state`, for one callback.
    pub const fn new(state: &'a WaylandState) -> Self {
        Self { state }
    }
}

impl core::fmt::Debug for WaylandCx<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("WaylandCx")
    }
}

impl PlatformCx for WaylandCx<'_> {
    fn create_surface(
        &self,
        attributes: &SurfaceAttributes,
    ) -> Result<Arc<dyn Surface>, PlatformError> {
        self.state
            .make_surface(attributes)
            .map(|surface| surface as Arc<dyn Surface>)
    }

    fn destroy_surface(&self, id: SurfaceId) {
        self.state.destroy_surface(id);
    }

    fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.state
            .live
            .surface(id)
            .map(|surface| surface as Arc<dyn Surface>)
    }

    fn surfaces(&self) -> Vec<Arc<dyn Surface>> {
        self.state
            .live
            .all()
            .into_iter()
            .map(|surface| surface as Arc<dyn Surface>)
            .collect()
    }

    fn monitors(&self) -> Vec<MonitorInfo> {
        self.state.live.monitors.borrow().clone()
    }

    fn primary_monitor(&self) -> Option<MonitorInfo> {
        // A compositor names no primary output, and inventing one — the first advertised, the
        // largest — would be a different answer on every desktop and on every reconnect.
        None
    }

    fn color_scheme(&self) -> Option<ColorScheme> {
        *self
            .state
            .live
            .scheme
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn clipboard(&self) -> &dyn Clipboard {
        &self.state.clipboard
    }

    fn capabilities(&self) -> &PlatformCapabilities {
        &self.state.live.capabilities
    }

    fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.state.live.clock) as Arc<dyn Clock>
    }

    fn waker(&self) -> Arc<dyn Waker> {
        Arc::clone(&self.state.live.waker) as Arc<dyn Waker>
    }

    fn scroll_settings(&self) -> zgui_platform::ScrollSettings {
        crate::input::desktop_scroll_settings()
    }

    fn request_exit(&self) {
        self.state.live.exiting.set(true);
        // The loop is parked on the socket until something happens, and nothing further will.
        self.state.live.waker.ping().ping();
    }

    fn is_exiting(&self) -> bool {
        self.state.live.exiting.get()
    }
}
