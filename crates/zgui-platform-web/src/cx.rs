//! The context a turn hands the application.

use std::sync::Arc;

use zgui_platform::{
    Clipboard, Clock, ColorScheme, MonitorInfo, PlatformCapabilities, PlatformCx, PlatformError,
    ScrollSettings, Surface, SurfaceAttributes, SurfaceId, Waker,
};

use crate::driver::Driver;

/// The page, for the length of one callback.
pub(crate) struct PageCx<'a> {
    /// The loop's state.
    pub(crate) driver: &'a Driver,
}

impl PlatformCx for PageCx<'_> {
    fn create_surface(
        &self,
        attributes: &SurfaceAttributes,
    ) -> Result<Arc<dyn Surface>, PlatformError> {
        self.driver.create_surface(attributes)
    }

    fn destroy_surface(&self, id: SurfaceId) {
        self.driver.destroy_surface(id);
    }

    fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.driver.surface(id)
    }

    fn surfaces(&self) -> Vec<Arc<dyn Surface>> {
        self.driver.surfaces()
    }

    fn monitors(&self) -> Vec<MonitorInfo> {
        Vec::new()
    }

    fn primary_monitor(&self) -> Option<MonitorInfo> {
        None
    }

    fn color_scheme(&self) -> Option<ColorScheme> {
        Some(self.driver.scheme())
    }

    fn clipboard(&self) -> &dyn Clipboard {
        self.driver.clipboard()
    }

    fn capabilities(&self) -> &PlatformCapabilities {
        self.driver.capabilities()
    }

    fn scroll_settings(&self) -> ScrollSettings {
        // A browser applies the person's scroll direction before a page sees the event, and
        // reports a notched wheel one detent at a time.
        ScrollSettings::desktop()
    }

    fn clock(&self) -> Arc<dyn Clock> {
        self.driver.clock()
    }

    fn waker(&self) -> Arc<dyn Waker> {
        self.driver.waker()
    }

    fn request_exit(&self) {
        self.driver.request_exit();
    }

    fn is_exiting(&self) -> bool {
        self.driver.is_exiting()
    }
}
