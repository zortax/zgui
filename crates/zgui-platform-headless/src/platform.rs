//! The platform itself: a clock, a clipboard, a waker and its offscreen surfaces.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use zgui_geom::{DevicePx, Size};
use zgui_platform::{
    Clipboard, Clock, ColorScheme, FocusGrabId, MonitorInfo, PlatformCapabilities, PlatformCx,
    PlatformError, ScrollSettings, Surface, SurfaceAttributes, SurfaceId, VirtualClock, WakeReason,
    Waker,
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
///
/// # Surface roles
///
/// Every [`SurfaceRole`](zgui_platform::SurfaceRole) makes an offscreen surface, whatever the
/// declared capabilities say, so a pop-up, a shell layer and a lock window are testable here. A
/// pop-up whose parent does not exist is refused with [`PlatformError::NoSuchSurface`]. A shell
/// layer that names an output the platform does not report is refused with
/// [`PlatformError::SurfaceCreation`]. A shell layer anchored to two opposite edges is as long as
/// its output on that axis, less its margins, and a lock window is as large as its output. The
/// placement is recorded: [`OffscreenSurface::role`], [`OffscreenSurface::layer_placement`] and
/// [`OffscreenSurface::popup_placement`] read it back.
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
    monitors: Mutex<Vec<MonitorInfo>>,
    /// The desktop's light or dark preference, when one has been declared.
    color_scheme: Mutex<Option<ColorScheme>>,
    /// What this platform says a scroll from its devices means.
    scroll: Mutex<ScrollSettings>,
    /// Whether the loop has been asked to finish.
    exiting: AtomicBool,
    /// Every focus grab that exists, with the surfaces it limits focus to.
    grabs: Mutex<Vec<(FocusGrabId, Vec<SurfaceId>)>>,
    /// The next focus grab number, never reused.
    next_grab: AtomicU64,
    /// Where the session lock is, as the application and a test moved it.
    lock: Mutex<SessionLockState>,
}

/// Where the session lock of a [`Headless`] platform is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionLockState {
    /// No lock was asked for, or the last one ended.
    #[default]
    Unlocked,
    /// The application asked for a lock and the test has not answered.
    Requested,
    /// The test confirmed the lock with [`Headless::confirm_lock`].
    Locked,
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
        // The platform stands in for a desktop, whose windows can be minimised, maximised and
        // closed. An application under test then draws the title bar it draws on a desktop.
        let mut capabilities = PlatformCapabilities::none();
        capabilities.window_controls = true;
        Self {
            clock: Arc::new(VirtualClock::new()),
            clipboard,
            capabilities,
            waker,
            surfaces: Mutex::new(Vec::new()),
            next_surface: AtomicU64::new(1),
            monitors: Mutex::new(Vec::new()),
            color_scheme: Mutex::new(None),
            scroll: Mutex::new(ScrollSettings::desktop()),
            exiting: AtomicBool::new(false),
            grabs: Mutex::new(Vec::new()),
            next_grab: AtomicU64::new(1),
            lock: Mutex::new(SessionLockState::Unlocked),
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
    pub fn with_monitors(self, monitors: Vec<MonitorInfo>) -> Self {
        *self.monitors.lock().expect("the outputs are not poisoned") = monitors;
        self
    }

    /// Replaces the outputs this platform reports, as a hotplug would, and wakes the loop with
    /// [`WakeReason::MonitorsChanged`].
    pub fn set_monitors(&self, monitors: Vec<MonitorInfo>) {
        *self.monitors.lock().expect("the outputs are not poisoned") = monitors;
        self.waker.wake(WakeReason::MonitorsChanged);
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

    /// The surfaces focus grab `grab` limits focus to, while it exists.
    pub fn focus_grab(&self, grab: FocusGrabId) -> Option<Vec<SurfaceId>> {
        self.grabs
            .lock()
            .expect("the grabs are not poisoned")
            .iter()
            .find(|(held, _)| *held == grab)
            .map(|(_, surfaces)| surfaces.clone())
    }

    /// Every focus grab that exists, oldest first.
    pub fn focus_grabs(&self) -> Vec<FocusGrabId> {
        self.grabs
            .lock()
            .expect("the grabs are not poisoned")
            .iter()
            .map(|(grab, _)| *grab)
            .collect()
    }

    /// Clears focus grab `grab`, as a desktop does when the user presses outside its surfaces,
    /// and wakes the loop with [`WakeReason::FocusGrabCleared`].
    pub fn clear_focus_grab(&self, grab: FocusGrabId) {
        let cleared = {
            let mut grabs = self.grabs.lock().expect("the grabs are not poisoned");
            grabs
                .iter_mut()
                .find(|(held, surfaces)| *held == grab && !surfaces.is_empty())
                .map(|(_, surfaces)| surfaces.clear())
                .is_some()
        };
        if cleared {
            self.waker.wake(WakeReason::FocusGrabCleared(grab));
        }
    }

    /// Where the session lock is.
    pub fn session_lock(&self) -> SessionLockState {
        *self.lock.lock().expect("the lock state is not poisoned")
    }

    /// Confirms the lock the application asked for, as a desktop does once it hides everything
    /// else, and wakes the loop with [`WakeReason::SessionLocked`].
    ///
    /// Does nothing unless a lock was asked for.
    pub fn confirm_lock(&self) {
        let mut lock = self.lock.lock().expect("the lock state is not poisoned");
        if *lock == SessionLockState::Requested {
            *lock = SessionLockState::Locked;
            drop(lock);
            self.waker.wake(WakeReason::SessionLocked);
        }
    }

    /// Refuses or ends the lock, as a desktop does, and wakes the loop with
    /// [`WakeReason::SessionLockFinished`].
    ///
    /// Does nothing while no lock is asked for or held.
    pub fn finish_lock(&self) {
        let mut lock = self.lock.lock().expect("the lock state is not poisoned");
        if *lock != SessionLockState::Unlocked {
            *lock = SessionLockState::Unlocked;
            drop(lock);
            self.waker.wake(WakeReason::SessionLockFinished);
        }
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
        let monitors = self.monitors();
        let size = crate::role::extent(attributes, &monitors, DEFAULT_SIZE, |parent| {
            self.offscreen(parent).is_some()
        })?;
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
        self.monitors
            .lock()
            .expect("the outputs are not poisoned")
            .clone()
    }

    fn primary_monitor(&self) -> Option<MonitorInfo> {
        self.monitors
            .lock()
            .expect("the outputs are not poisoned")
            .first()
            .cloned()
    }

    fn create_focus_grab(&self) -> Option<FocusGrabId> {
        if !self.capabilities.focus_grab {
            return None;
        }
        let grab = FocusGrabId::new(self.next_grab.fetch_add(1, Ordering::Relaxed));
        self.grabs
            .lock()
            .expect("the grabs are not poisoned")
            .push((grab, Vec::new()));
        Some(grab)
    }

    fn set_focus_grab(&self, grab: FocusGrabId, surfaces: &[SurfaceId]) {
        let existing: Vec<SurfaceId> = surfaces
            .iter()
            .copied()
            .filter(|surface| self.offscreen(*surface).is_some())
            .collect();
        if let Some((_, held)) = self
            .grabs
            .lock()
            .expect("the grabs are not poisoned")
            .iter_mut()
            .find(|(held, _)| *held == grab)
        {
            *held = existing;
        }
    }

    fn destroy_focus_grab(&self, grab: FocusGrabId) {
        self.grabs
            .lock()
            .expect("the grabs are not poisoned")
            .retain(|(held, _)| *held != grab);
    }

    fn lock_session(&self) -> Result<(), PlatformError> {
        if !self.capabilities.session_lock {
            return Err(zgui_platform::Unsupported.into());
        }
        let mut lock = self.lock.lock().expect("the lock state is not poisoned");
        if *lock != SessionLockState::Unlocked {
            return Err(PlatformError::Backend(
                "this application holds a session lock already".to_owned(),
            ));
        }
        *lock = SessionLockState::Requested;
        Ok(())
    }

    fn unlock_session(&self) {
        let mut lock = self.lock.lock().expect("the lock state is not poisoned");
        if *lock == SessionLockState::Locked {
            *lock = SessionLockState::Unlocked;
        }
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
    fn a_shell_layer_and_its_pop_up_are_plain_surfaces_whose_placement_is_recorded() {
        use zgui_geom::{Edges, Point, Rect};
        use zgui_platform::{Layer, LayerPatch, LayerPlacement, PopupPlacement, SurfaceRole};

        let platform = Headless::new();
        let bar = LayerPlacement::new(Layer::Top).with_anchors(Edges::new(true, true, false, true));
        let mut attributes =
            SurfaceAttributes::new("bar").with_size(Size::new(CssPx(0.0), CssPx(34.0)));
        attributes.role = SurfaceRole::Layer(bar.clone());
        let surface = platform
            .create_surface(&attributes)
            .expect("a plain surface");
        assert_eq!(
            surface.size().width.0,
            640.0,
            "stretched across the default extent"
        );

        surface
            .set_layer_placement(&LayerPatch::new().with_layer(Layer::Overlay))
            .expect("a layer takes a patch");
        let offscreen = platform.offscreen(surface.id()).expect("the same surface");
        let placed = offscreen.layer_placement().expect("a layer");
        assert_eq!(placed.layer, Layer::Overlay);
        assert_eq!(placed.anchors, bar.anchors);
        assert_eq!(offscreen.popup_placement(), None);

        let rect = Rect::new(
            Point::new(CssPx(4.0), CssPx(0.0)),
            Size::new(CssPx(20.0), CssPx(34.0)),
        );
        let mut menu = SurfaceAttributes::new("menu");
        menu.role = SurfaceRole::Popup(PopupPlacement::below(surface.id(), rect));
        let popup = platform.create_surface(&menu).expect("the parent exists");
        let placed = platform
            .offscreen(popup.id())
            .and_then(|popup| popup.popup_placement())
            .expect("a pop-up");
        assert_eq!(placed.parent, surface.id());
        assert_eq!(placed.anchor_rect, rect);
        assert!(popup.set_layer_placement(&LayerPatch::new()).is_err());

        platform.destroy_surface(surface.id());
        assert!(
            platform.create_surface(&menu).is_err(),
            "a pop-up of a surface that is gone is refused"
        );
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

    #[test]
    fn a_session_lock_is_asked_for_confirmed_and_unlocked() {
        use super::SessionLockState;
        use zgui_platform::{PlatformCapabilities, WakeReason};

        let platform = Headless::new();
        assert!(platform.lock_session().is_err(), "no capability, no lock");

        let mut capabilities = PlatformCapabilities::none();
        capabilities.session_lock = true;
        let platform = Headless::new().with_capabilities(capabilities);
        platform.lock_session().expect("the lock is asked for");
        assert_eq!(platform.session_lock(), SessionLockState::Requested);
        assert!(platform.lock_session().is_err(), "one lock at a time");

        platform.unlock_session();
        assert_eq!(
            platform.session_lock(),
            SessionLockState::Requested,
            "an unconfirmed lock cannot be unlocked"
        );
        platform.confirm_lock();
        assert!(matches!(
            platform.drain_wakes()[..],
            [WakeReason::SessionLocked]
        ));
        platform.unlock_session();
        assert_eq!(platform.session_lock(), SessionLockState::Unlocked);

        platform.lock_session().expect("a new lock after an unlock");
        platform.finish_lock();
        assert_eq!(platform.session_lock(), SessionLockState::Unlocked);
        assert!(matches!(
            platform.drain_wakes()[..],
            [WakeReason::SessionLockFinished]
        ));
    }
}
