//! What the platform offers, for the duration of one callback.

use std::cell::{Cell, OnceCell, RefCell};
use std::sync::Arc;

use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::WindowId;
use zgui_platform::{
    Clipboard, ClipboardFormat, Clock, ColorScheme, DecorationSource, MonitorInfo,
    PlatformCapabilities, PlatformCx, PlatformError, ScrollSettings, Surface, SurfaceAttributes,
    SurfaceId, Waker,
};

use crate::clipboard::DesktopClipboard;
use crate::clock::SystemClock;
use crate::monitor;
use crate::surface::{WinitSurface, window_attributes};
use crate::theme;
use crate::waker::{ProxyWaker, UserEvent};

/// Everything that outlives a callback: the clock, the clipboards, the windows, the way in.
///
/// This is the loop's own state, held beside it rather than inside the context, because the
/// context is borrowed for the length of one callback and none of this may be.
pub(crate) struct Shared {
    /// The clock every phase reads.
    clock: Arc<SystemClock>,
    /// The desktop's clipboards.
    clipboard: DesktopClipboard,
    /// How another thread reaches this loop.
    waker: Arc<ProxyWaker>,
    /// The channel the accessibility adapters report through.
    proxy: EventLoopProxy<UserEvent>,
    /// What this desktop can and cannot do, settled the first time it is asked.
    capabilities: OnceCell<PlatformCapabilities>,
    /// The windows that exist.
    surfaces: RefCell<Vec<Arc<WinitSurface>>>,
    /// The next surface number, never reused.
    next: Cell<u64>,
    /// The desktop's light or dark preference, as last discovered.
    scheme: Cell<Option<ColorScheme>>,
}

impl Shared {
    /// The state a loop starts with, with the way in already open.
    pub(crate) fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            clock: Arc::new(SystemClock::new()),
            clipboard: DesktopClipboard::new(),
            waker: Arc::new(ProxyWaker::new(proxy.clone())),
            proxy,
            capabilities: OnceCell::new(),
            surfaces: RefCell::new(Vec::new()),
            next: Cell::new(1),
            scheme: Cell::new(None),
        }
    }

    /// Discovers what this desktop can do, and hands the clipboards its connection.
    ///
    /// Both need the running loop, which does not exist until the first callback, so neither can be
    /// settled in the constructor.
    pub(crate) fn attach(&self, event_loop: &ActiveEventLoop) {
        self.scheme
            .set(event_loop.system_theme().map(theme::scheme));
        let _ = self
            .capabilities
            .set(capabilities(event_loop, self.scheme.get().is_some()));
        self.clipboard
            .attach(event_loop, Arc::clone(&self.waker) as Arc<dyn Waker>);
    }

    /// The clock, as the contract's view of it.
    pub(crate) fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.clock) as Arc<dyn Clock>
    }

    /// How another thread reaches this loop.
    pub(crate) fn waker(&self) -> Arc<dyn Waker> {
        Arc::clone(&self.waker) as Arc<dyn Waker>
    }

    /// The surface drawn into the window `id` names, while it still exists.
    pub(crate) fn by_window(&self, id: WindowId) -> Option<Arc<WinitSurface>> {
        self.surfaces
            .borrow()
            .iter()
            .find(|surface| surface.window().id() == id)
            .map(Arc::clone)
    }

    /// Forgets a window that has gone.
    pub(crate) fn forget(&self, id: WindowId) {
        self.surfaces
            .borrow_mut()
            .retain(|surface| surface.window().id() != id);
    }

    /// Forgets every window, for the platform lifecycle that takes them all away at once.
    pub(crate) fn forget_all(&self) {
        self.surfaces.borrow_mut().clear();
    }

    /// Records the desktop's light or dark preference.
    pub(crate) fn set_scheme(&self, scheme: ColorScheme) {
        self.scheme.set(Some(scheme));
    }
}

/// The platform, for the length of one callback.
///
/// It is handed in and never stored, and that restriction is the platform's rather than a
/// preference: the object that can create windows and read outputs is valid only while the loop is
/// inside a callback, and is neither shareable nor sendable. The three things that genuinely
/// outlive a callback — a surface, the clock and the waker — say so in their own types.
pub struct WinitCx<'a> {
    /// Everything that outlives this callback.
    shared: &'a Shared,
    /// The loop, for the length of this callback.
    event_loop: &'a ActiveEventLoop,
}

impl<'a> WinitCx<'a> {
    /// The context for one callback of `event_loop`.
    pub(crate) const fn new(shared: &'a Shared, event_loop: &'a ActiveEventLoop) -> Self {
        Self { shared, event_loop }
    }
}

impl PlatformCx for WinitCx<'_> {
    fn create_surface(
        &self,
        attributes: &SurfaceAttributes,
    ) -> Result<Arc<dyn Surface>, PlatformError> {
        let window = self
            .event_loop
            .create_window(window_attributes(attributes, self.shared.scheme.get()))
            .map_err(|error| PlatformError::SurfaceCreation(error.to_string()))?;
        let window = Arc::new(window);

        let id = SurfaceId::new(self.shared.next.get());
        self.shared.next.set(self.shared.next.get() + 1);
        let surface = Arc::new(WinitSurface::new(id, Arc::clone(&window)));

        // Attached here and nowhere else. The adapter refuses a window that has already been shown,
        // and a surface is created hidden precisely so that this can happen before the first frame
        // makes it visible — there is no second chance at it later.
        surface
            .a11y()
            .attach(accesskit_winit::Adapter::with_event_loop_proxy(
                self.event_loop,
                &window,
                self.shared.proxy.clone(),
            ));

        self.shared.surfaces.borrow_mut().push(Arc::clone(&surface));
        Ok(surface as Arc<dyn Surface>)
    }

    fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.shared
            .surfaces
            .borrow()
            .iter()
            .find(|surface| Surface::id(surface.as_ref()) == id)
            .map(|surface| Arc::clone(surface) as Arc<dyn Surface>)
    }

    fn surfaces(&self) -> Vec<Arc<dyn Surface>> {
        self.shared
            .surfaces
            .borrow()
            .iter()
            .map(|surface| Arc::clone(surface) as Arc<dyn Surface>)
            .collect()
    }

    fn monitors(&self) -> Vec<MonitorInfo> {
        self.event_loop
            .available_monitors()
            .map(|handle| monitor::describe(&handle))
            .collect()
    }

    fn primary_monitor(&self) -> Option<MonitorInfo> {
        self.event_loop
            .primary_monitor()
            .map(|handle| monitor::describe(&handle))
    }

    fn color_scheme(&self) -> Option<ColorScheme> {
        // Absent means unknown and never light: guessing light on a desktop that cannot be asked
        // shows every user who chose dark a white flash at every launch.
        self.shared.scheme.get()
    }

    fn clipboard(&self) -> &dyn Clipboard {
        &self.shared.clipboard
    }

    fn capabilities(&self) -> &PlatformCapabilities {
        // Settled once and never again, which is what lets the contract's accessor be a borrow
        // rather than a clone on every question a component asks. It is normally settled before the
        // first callback; asking here as well means a caller that somehow got in first is answered
        // with the truth rather than with the empty set.
        self.shared
            .capabilities
            .get_or_init(|| capabilities(self.event_loop, self.shared.scheme.get().is_some()))
    }

    fn scroll_settings(&self) -> ScrollSettings {
        crate::input::scrolling::desktop_scroll_settings()
    }

    fn clock(&self) -> Arc<dyn Clock> {
        self.shared.clock()
    }

    fn waker(&self) -> Arc<dyn Waker> {
        self.shared.waker()
    }

    fn request_exit(&self) {
        self.event_loop.exit();
    }

    fn is_exiting(&self) -> bool {
        self.event_loop.exiting()
    }
}

/// What this desktop can actually do.
///
/// Built up from nothing rather than down from everything, so that a capability nobody filled in
/// reads as absent. An interface that degrades because it was told something is missing is a
/// working interface; one that offers a command the desktop can never run is not.
fn capabilities(event_loop: &ActiveEventLoop, knows_scheme: bool) -> PlatformCapabilities {
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

    let placed_by_the_application = event_loop
        .display_handle()
        .is_ok_and(|handle| !matches!(handle.as_raw(), RawDisplayHandle::Wayland(_)));

    let mut capabilities = PlatformCapabilities::none();
    // A window can be dropped on, and what arrives is always paths.
    capabilities
        .drop_mime_types
        .push("text/uri-list".to_owned());
    capabilities.clipboard_formats = vec![ClipboardFormat::Text];
    capabilities.clipboard_primary_selection = cfg!(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ));
    capabilities.ime = true;
    capabilities.ime_purpose_hints = true;
    // A compositor that forbids a window from placing itself also forbids a pop-up placed in
    // desktop coordinates, so these two answer together and an overlay is drawn in-window.
    capabilities.absolute_window_position = placed_by_the_application;
    capabilities.window_levels = placed_by_the_application;
    capabilities.decorations = DecorationSource::Platform;
    capabilities.system_color_scheme = knows_scheme;
    capabilities
}
