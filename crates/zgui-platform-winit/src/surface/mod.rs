//! One window: what is drawn into it, what it looks like, and who is listening to it.

mod a11y;
mod attributes;
mod chrome;
mod handles;

pub(crate) use crate::surface::attributes::window as window_attributes;

use std::sync::Arc;

use accesskit::TreeUpdate;
use winit::dpi::LogicalSize;
use winit::window::Window;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::{
    CursorStyle, FullscreenMode, GpuSurface, ResizeEdge, Surface, SurfaceId, TextInput, Unsupported,
};

use crate::surface::a11y::A11y;

/// A window, seen as something to draw into and interact with.
///
/// It is shared and thread-safe because the one operation that has to work from anywhere is asking
/// for a frame: work finishing on a worker thread is one of the four things that can want one, and
/// making it hop to the main thread first would put a scheduling delay in front of every frame the
/// application ever draws.
///
/// # Two spaces, and they are not interchangeable
///
/// [`Surface::size`] answers in physical pixels because that is what a swap chain is allocated in,
/// and a rounding error there is a visibly stretched frame. Everything a *caller* asks for is in
/// CSS pixels because that is the space a stylesheet is written in. The bridge is this window's own
/// scale — never the monitor's, which can differ, and which is the difference that only shows up on
/// the mixed-density arrangements nobody has to hand.
pub struct WinitSurface {
    /// Which surface this is, in the contract's numbering.
    id: SurfaceId,
    /// The window itself.
    window: Arc<Window>,
    /// The accessibility channel, once one has been attached.
    a11y: A11y,
}

impl WinitSurface {
    /// A surface over `window`, numbered `id`, with nothing listening to it yet.
    pub(crate) fn new(id: SurfaceId, window: Arc<Window>) -> Self {
        Self {
            id,
            window,
            a11y: A11y::default(),
        }
    }

    /// The window underneath, for the parts of the loop that have to name it.
    pub(crate) fn window(&self) -> &Arc<Window> {
        &self.window
    }

    /// The accessibility channel.
    pub(crate) fn a11y(&self) -> &A11y {
        &self.a11y
    }

    /// The scale this window is presented at, never zero.
    ///
    /// A compositor is allowed to answer with nonsense while a window is being mapped, and every
    /// conversion out of physical pixels divides by this. One is the only answer that leaves the
    /// numbers finite.
    pub(crate) fn scale(&self) -> f64 {
        let scale = self.window.scale_factor();
        if scale > 0.0 { scale } else { 1.0 }
    }
}

impl core::fmt::Debug for WinitSurface {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WinitSurface")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Surface for WinitSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn size(&self) -> Size<DevicePx, Device> {
        let size = self.window.inner_size();
        Size::new(DevicePx(size.width as f32), DevicePx(size.height as f32))
    }

    fn scale_factor(&self) -> f64 {
        self.scale()
    }

    fn refresh_rate_millihertz(&self) -> Option<u32> {
        self.window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .filter(|rate| *rate > 0)
    }

    fn request_size(&self, size: Size<CssPx, Css>) -> Option<Size<DevicePx, Device>> {
        self.window
            .request_inner_size(LogicalSize::new(size.width.0, size.height.0))
            .map(|taken| Size::new(DevicePx(taken.width as f32), DevicePx(taken.height as f32)))
    }

    fn set_min_size(&self, size: Option<Size<CssPx, Css>>) {
        self.window
            .set_min_inner_size(size.map(|size| LogicalSize::new(size.width.0, size.height.0)));
    }

    fn set_max_size(&self, size: Option<Size<CssPx, Css>>) {
        self.window
            .set_max_inner_size(size.map(|size| LogicalSize::new(size.width.0, size.height.0)));
    }

    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn pre_present_notify(&self) {
        self.window.pre_present_notify();
    }

    fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    fn set_visible(&self, visible: bool) {
        self.window.set_visible(visible);
    }

    fn set_decorated(&self, decorated: bool) {
        self.window.set_decorations(decorated);
    }

    fn set_resizable(&self, resizable: bool) {
        self.window.set_resizable(resizable);
    }

    fn set_maximized(&self, maximized: bool) {
        self.window.set_maximized(maximized);
    }

    fn set_minimized(&self, minimized: bool) {
        self.window.set_minimized(minimized);
    }

    fn set_fullscreen(&self, mode: Option<FullscreenMode>) {
        self.window
            .set_fullscreen(mode.map(|mode| chrome::fullscreen(&self.window, mode)));
    }

    fn begin_move_drag(&self) -> Result<(), Unsupported> {
        self.window.drag_window().map_err(|_| Unsupported)
    }

    fn begin_resize_drag(&self, edge: ResizeEdge) -> Result<(), Unsupported> {
        self.window
            .drag_resize_window(chrome::resize(edge))
            .map_err(|_| Unsupported)
    }

    fn set_cursor(&self, cursor: CursorStyle) {
        match chrome::cursor(cursor) {
            Some(icon) => {
                self.window.set_cursor(icon);
                self.window.set_cursor_visible(true);
            }
            None => self.window.set_cursor_visible(false),
        }
    }

    fn set_pointer_passthrough(&self, passthrough: bool) -> Result<(), Unsupported> {
        self.window
            .set_cursor_hittest(!passthrough)
            .map_err(|_| Unsupported)
    }

    fn set_text_input(&self, state: Option<TextInput>) {
        // The order matters in one direction only: an input method told where the caret is while
        // it believes the field is closed places its candidate window over the text being composed.
        // Setting the area first and the flag second is the order that cannot produce that.
        if let Some(state) = state {
            self.window.set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(state.caret_origin.x.0, state.caret_origin.y.0),
                LogicalSize::new(state.caret_size.width.0, state.caret_size.height.0),
            );
            self.window
                .set_ime_purpose(crate::input::ime::purpose(state.purpose));
        }
        self.window.set_ime_allowed(state.is_some());
    }

    fn reset_dead_keys(&self) {
        self.window.reset_dead_keys();
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
