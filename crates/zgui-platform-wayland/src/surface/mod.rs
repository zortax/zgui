//! One surface: what is drawn into it, what it looks like, and what it owes the compositor.

pub mod a11y;
pub mod chrome;
pub mod fractional;
mod handles;
pub mod role;
pub mod scale;
pub mod seat;
pub mod state;

pub use crate::surface::a11y::A11y;
pub use crate::surface::fractional::Fractional;
pub use crate::surface::role::Role;
pub use crate::surface::scale::Scale;
pub use crate::surface::seat::SeatLink;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use accesskit::TreeUpdate;
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_surface::WlSurface;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::{
    CursorStyle, Decorations, FullscreenMode, GpuSurface, PresentPacing, PresentationTiming,
    Surface, SurfaceId, TextInput, Unsupported,
};

use crate::driver::WaylandState;
use crate::frame::Presentation;
use crate::surface::state::{Bounds, Shared};

/// What a surface needs from the loop it belongs to.
///
/// Every one of these is a handle the loop already holds and hands over by cloning: the connection
/// a graphics API is given, the queue everything this surface creates is dispatched on, the
/// compositor its regions come from, the global its presentation is reported by, and the pipe that
/// ends the loop's park. They travel together because they are handed over together, once.
#[derive(Clone)]
pub struct Links {
    /// The connection everything is created on.
    pub conn: Connection,
    /// The queue everything is dispatched on.
    pub qh: QueueHandle<WaylandState>,
    /// The compositor, for the regions a surface builds.
    pub compositor: WlCompositor,
    /// Where a request for presentation feedback goes.
    pub presentation: Presentation,
    /// How a request made away from the loop's thread reaches it.
    pub ping: calloop::ping::Ping,
    /// How a surface reaches the seat.
    pub seat: Arc<SeatLink>,
    /// Where an assistive technology's questions are answered.
    pub waker: Arc<dyn zgui_platform::Waker>,
    /// The shell a pop-up's later requests are made against.
    pub shell: Arc<crate::surface::role::xdg::XdgShell>,
}

impl core::fmt::Debug for Links {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Links")
    }
}

/// A Wayland surface, seen as something to draw into and interact with.
///
/// Shared and thread-safe, because [`Surface::request_redraw`] has to work from anywhere: work
/// finishing on a worker thread is one of the four things that can want a frame, and making it hop
/// to the loop's thread first would put a scheduling delay in front of every one of them.
///
/// That is affordable here because a protocol object is itself shareable — a request from any
/// thread is serialised into the connection's own buffer — so most of this type forwards straight
/// to the compositor with no hop at all. What cannot be done from another thread is *flushing*
/// that buffer, which only the loop does, so a request made from elsewhere is followed by a poke.
pub struct WaylandSurface {
    /// Which surface this is, in the contract's numbering.
    id: SurfaceId,
    /// The shell object it is.
    role: Role,
    /// Fractional scaling, when the compositor offers it.
    fractional: Fractional,
    /// Everything about it that changes.
    shared: Mutex<Shared>,
    /// A redraw asked for from anywhere, waiting to be pulled into the pacer.
    asked: AtomicBool,
    /// Whether the compositor has configured this surface at least once.
    mapped: AtomicBool,
    /// How a request made away from the loop's thread reaches it.
    ping: calloop::ping::Ping,
    /// The connection, for the display handle a graphics API needs.
    conn: Connection,
    /// The queue everything this surface creates is dispatched on.
    qh: QueueHandle<WaylandState>,
    /// The compositor, for the regions this surface builds.
    compositor: WlCompositor,
    /// Where a request for presentation feedback goes.
    presentation: Presentation,
    /// The size the application asked for, kept for the first configure.
    wanted: Mutex<Size<CssPx, Css>>,
    /// Where a pop-up was placed, for the reposition that follows it growing.
    placement: Option<zgui_platform::PopupPlacement>,
    /// The extent the pop-up's parent was at when it was placed.
    parent_extent: Size<CssPx, Css>,
    /// How many times this pop-up has been moved, which is how an answer is matched to a request.
    reposition_token: std::sync::atomic::AtomicU32,
    /// The shell a reposition is made against.
    shell: Arc<crate::surface::role::xdg::XdgShell>,
    /// The channel an assistive technology talks to.
    a11y: A11y,
    /// The seat's own requests, which belong to the loop rather than to this surface.
    ///
    /// A cursor shape and an interactive drag are both asked of a *seat*, and there is one seat
    /// whichever surface asks. So the loop lends each surface a way to reach it, guarded by
    /// whether that surface is the one the pointer is actually on.
    seat: Arc<SeatLink>,
}

impl WaylandSurface {
    /// A surface over `role`, numbered `id`, wired into the loop by `links`.
    pub(crate) fn new(
        id: SurfaceId,
        role: Role,
        fractional: Fractional,
        wanted: Size<CssPx, Css>,
        placement: Option<zgui_platform::PopupPlacement>,
        parent_extent: Size<CssPx, Css>,
        links: Links,
    ) -> Arc<Self> {
        Arc::new(Self {
            id,
            role,
            fractional,
            shared: Mutex::new(Shared::new()),
            asked: AtomicBool::new(false),
            mapped: AtomicBool::new(false),
            ping: links.ping,
            conn: links.conn,
            qh: links.qh,
            compositor: links.compositor,
            presentation: links.presentation,
            wanted: Mutex::new(wanted),
            placement,
            parent_extent,
            reposition_token: std::sync::atomic::AtomicU32::new(1),
            shell: links.shell,
            a11y: A11y::open(id, links.waker),
            seat: links.seat,
        })
    }

    /// The shell object this surface is.
    pub const fn role(&self) -> &Role {
        &self.role
    }

    /// The surface underneath.
    pub fn wl_surface(&self) -> &WlSurface {
        self.role.wl_surface()
    }

    /// The connection this surface belongs to.
    pub const fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The changing half, recovering from a panic on another thread.
    ///
    /// A poisoned lock means a thread panicked while reading a size. Everything under it is plain
    /// data that cannot be left half-written, and refusing to draw ever again turns one thread's
    /// panic into a window that has stopped.
    pub(crate) fn shared(&self) -> MutexGuard<'_, Shared> {
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The extent the application asked for, which the first configure may or may not honour.
    pub(crate) fn wanted(&self) -> Size<CssPx, Css> {
        *self
            .wanted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Fractional scaling's objects, for the commit that carries a new extent.
    pub(crate) const fn fractional(&self) -> &Fractional {
        &self.fractional
    }

    /// Takes the redraw asked for from anywhere since the last turn.
    pub(crate) fn take_request(&self) -> bool {
        self.asked.swap(false, Ordering::AcqRel)
    }

    /// How this surface reaches the seat.
    pub(crate) const fn seat(&self) -> &Arc<SeatLink> {
        &self.seat
    }

    /// The channel an assistive technology talks to.
    pub(crate) const fn a11y(&self) -> &A11y {
        &self.a11y
    }

    /// Where this pop-up was asked to be placed, when it is one.
    pub(crate) const fn placement(&self) -> Option<&zgui_platform::PopupPlacement> {
        self.placement.as_ref()
    }

    /// Records that a pop-up has been configured, which is what makes it movable.
    ///
    /// Moving one that has never been configured is a protocol error, and a menu asked to
    /// reposition itself in the same breath as it opened would hit it.
    pub(crate) fn popup_mapped(&self) {
        self.mapped.store(true, Ordering::Release);
    }

    /// Whether a pop-up has been configured at least once.
    pub(crate) fn is_mapped(&self) -> bool {
        self.mapped.load(Ordering::Acquire)
    }

    /// Asks the compositor to place this pop-up again, at `size`.
    ///
    /// Answers with whether it could. A shell too old to move a pop-up, or one the compositor has
    /// not configured yet, is left where it is — moving an unmapped pop-up is a protocol error, and
    /// the caller's fallback is to close it and open another.
    pub(crate) fn reposition(&self, size: Size<CssPx, Css>) -> bool {
        let Some(popup) = self.role.popup() else {
            return false;
        };
        let Some(placement) = self.placement() else {
            return false;
        };
        let token = self.reposition_token.fetch_add(1, Ordering::Relaxed);
        popup.reposition(
            &self.shell,
            &self.qh,
            &crate::surface::role::xdg::popup::Placed {
                place: placement,
                parent: self.parent_extent,
                size,
            },
            token,
            self.is_mapped(),
        )
    }

    /// The smallest and largest extents the user may drag to.
    pub(crate) fn bounds(&self) -> Bounds {
        self.shared().bounds
    }

    /// Records both bounds and states them together.
    pub(crate) fn set_bounds(&self, bounds: Bounds) {
        self.shared().bounds = bounds;
        if let Some(window) = self.role.window() {
            window.set_bounds(bounds.0, bounds.1);
        }
    }

    /// An empty region, for a surface that wants presses to reach whatever is behind it.
    ///
    /// The compositor copies a region when it is set rather than referencing it, so the object is
    /// retired straight away and nothing holds it.
    pub(crate) fn empty_region(&self) -> WlRegion {
        self.compositor.create_region(&self.qh, ())
    }

    /// Asks the compositor to report when the frame about to be committed reaches the screen.
    pub(crate) fn ask_for_feedback(&self) {
        self.presentation.ask(&self.qh, self.wl_surface(), self.id);
    }

    /// Asks the compositor to tell us when it is ready for the next frame.
    ///
    /// Requested before the commit that carries a frame, so that it rides that commit. A callback
    /// asked for after the commit belongs to the *next* one, which never comes: that is the shape
    /// of a client that draws one frame and stops.
    pub(crate) fn ask_for_callback(&self) {
        let surface = self.wl_surface();
        surface.frame(&self.qh, surface.clone());
    }

    /// Ends a delivered redraw, keeping the frame chain alive whether or not anything was drawn.
    ///
    /// A frame callback rides a commit. Every reason a frame can end without presenting — no
    /// damage, a lost device, a refused acquisition, a surface the compositor is not drawing —
    /// would otherwise end the turn with no commit, and the compositor would never speak about
    /// this surface again.
    pub(crate) fn finish_redraw(&self, now: Instant) {
        let presented = {
            let mut shared = self.shared();
            shared.pacer.committed(now);
            std::mem::replace(&mut shared.presented, false)
        };
        if !presented {
            self.ask_for_callback();
            self.role.commit();
        }
    }
}

impl Drop for WaylandSurface {
    /// Destroys the objects that hang off the surface before the surface itself.
    ///
    /// Order is not a preference: a viewport outliving the surface it maps is a protocol error
    /// rather than a leak. This runs before any field is dropped, and the surface is owned by the
    /// role, which is a field — so the ordering holds wherever the last handle happens to go.
    fn drop(&mut self) {
        // Before the objects, because the channel is on another thread and a question answered
        // against a surface that has gone is answered against nothing.
        self.a11y.close();
        self.fractional.release();
    }
}

impl core::fmt::Debug for WaylandSurface {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WaylandSurface")
            .field("id", &self.id)
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

impl Surface for WaylandSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn size(&self) -> Size<DevicePx, Device> {
        self.shared().size
    }

    fn scale_factor(&self) -> f64 {
        self.shared().scale
    }

    fn refresh_rate_millihertz(&self) -> Option<u32> {
        self.shared().timing.refresh_rate_millihertz()
    }

    fn present_pacing(&self) -> PresentPacing {
        // The whole reason this backend exists. Presentation that waits for the display waits on
        // the frame callbacks this backend is already waiting on, on the thread that also reads
        // input, and a surface the compositor stops drawing blocks it for a second at a time.
        PresentPacing::Platform
    }

    fn presentation_timing(&self) -> Option<PresentationTiming> {
        Some(self.shared().snapshot())
    }

    fn request_size(&self, size: Size<CssPx, Css>) -> Option<Size<DevicePx, Device>> {
        chrome::request_size(self, size)
    }

    fn set_min_size(&self, size: Option<Size<CssPx, Css>>) {
        chrome::set_min_size(self, size);
    }

    fn set_max_size(&self, size: Option<Size<CssPx, Css>>) {
        chrome::set_max_size(self, size);
    }

    fn request_redraw(&self) {
        // Coalescing, and cheap enough to be called from a worker thread on every value that
        // changed: the flag is what the loop reads, and the poke is only there to end a park.
        if !self.asked.swap(true, Ordering::AcqRel) {
            zgui_profile::latency::mark("req.redraw");
        }
        self.ping.ping();
    }

    fn pre_present_notify(&self) {
        chrome::pre_present(self);
    }

    fn set_title(&self, title: &str) {
        if let Some(window) = self.role.window() {
            window.set_title(title);
        }
    }

    fn set_visible(&self, visible: bool) {
        chrome::set_visible(self, visible);
    }

    fn set_decorations(&self, decorations: Decorations) {
        if let Some(window) = self.role.window() {
            window.request_decorations(decorations);
        }
    }

    fn set_resizable(&self, resizable: bool) {
        chrome::set_resizable(self, resizable);
    }

    fn set_maximized(&self, maximized: bool) {
        if let Some(window) = self.role.window() {
            window.set_maximized(maximized);
        }
    }

    fn set_minimized(&self, minimized: bool) {
        // There is no way back: a compositor tells a client nothing about being minimised and
        // offers no request to undo it. Restoring is the person's, through the desktop.
        if minimized && let Some(window) = self.role.window() {
            window.set_minimized();
        }
    }

    fn set_fullscreen(&self, mode: Option<FullscreenMode>) {
        let Some(window) = self.role.window() else {
            return;
        };
        // A compositor never changes an output's mode for a client, so both kinds of full screen
        // are the same request and the exclusive one is the borderless one.
        window.set_fullscreen(mode.is_some());
    }

    fn is_maximized(&self) -> bool {
        self.shared().maximized
    }

    fn fullscreen(&self) -> Option<FullscreenMode> {
        self.shared()
            .fullscreen
            .then_some(FullscreenMode::Borderless)
    }

    fn focus(&self) {
        self.seat.activate(self.id, true);
    }

    fn request_attention(&self, urgent: bool) {
        // Turning it off is not a request this protocol has: a desktop stops drawing attention when
        // the person looks at the window, which is its decision and not the application's.
        if urgent {
            self.seat.activate(self.id, false);
        }
    }

    fn set_cursor(&self, cursor: CursorStyle) {
        // The pointer belongs to the seat rather than to a surface, and there is one of it. What a
        // surface asks for is what the pointer looks like while it is over that surface, so the
        // request is refused unless the pointer is actually there — otherwise a background window
        // changing its cursor would change it under the window the person is using.
        self.seat.set_cursor(self.id, cursor);
    }

    fn set_pointer_passthrough(&self, passthrough: bool) -> Result<(), Unsupported> {
        chrome::set_pointer_passthrough(self, passthrough)
    }

    fn begin_move_drag(&self) -> Result<(), Unsupported> {
        chrome::begin_move_drag(self)
    }

    fn begin_resize_drag(&self, edge: zgui_platform::ResizeEdge) -> Result<(), Unsupported> {
        chrome::begin_resize_drag(self, edge)
    }

    fn set_text_input(&self, state: Option<TextInput>) {
        self.seat.set_text_input(self.id, state);
    }

    fn reset_dead_keys(&self) {
        self.seat.reset_composition();
    }

    fn push_a11y_update(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        self.a11y.publish(build);
    }

    fn gpu(&self) -> Option<&dyn GpuSurface> {
        Some(self)
    }

    fn gpu_shared(self: Arc<Self>) -> Option<Arc<dyn GpuSurface>> {
        Some(self)
    }
}
