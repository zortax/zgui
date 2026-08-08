//! One window: what is drawn into it, what it looks like, and who is listening to it.

pub(crate) mod a11y;
mod attributes;
mod chrome;
mod handles;

pub(crate) use crate::surface::attributes::window as window_attributes;

use std::sync::Arc;

use accesskit::TreeUpdate;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::Window;
use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui_platform::{
    ColorScheme, CursorStyle, FullscreenMode, GpuSurface, ResizeEdge, Surface, SurfaceId,
    TextInput, Unsupported, WindowIcon, WindowLevel,
};

/// A window, seen as something to draw into and interact with.
///
/// It is shared and thread-safe because the one operation that has to work from anywhere is asking
/// for a frame: work finishing on a worker thread is one of the four things that can want one, and
/// making it hop to the main thread first would put a scheduling delay in front of every frame the
/// application ever draws.
///
/// That is also why the accessibility adapter is not in here. It cannot leave the thread that made
/// it, so it stays on that thread and this names it by the surface's own number.
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
}

impl WinitSurface {
    /// A surface over `window`, numbered `id`, with nothing listening to it yet.
    pub(crate) const fn new(id: SurfaceId, window: Arc<Window>) -> Self {
        Self { id, window }
    }

    /// The window underneath, for the parts of the loop that have to name it.
    pub(crate) fn window(&self) -> &Arc<Window> {
        &self.window
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
            .field("a11y", &a11y::is_attached(self.id))
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

    fn set_position(&self, position: Point<CssPx, Css>) {
        // Winit answers a compositor that places windows itself by doing nothing here, which is the
        // no-op this contract asks for; nothing extra is needed to make it silent.
        self.window
            .set_outer_position(LogicalPosition::new(position.x.0, position.y.0));
    }

    fn position(&self) -> Option<Point<CssPx, Css>> {
        let scale = self.window.scale_factor();
        self.window.outer_position().ok().map(|position| {
            let position: LogicalPosition<f64> = position.to_logical(scale);
            Point::new(CssPx(position.x as f32), CssPx(position.y as f32))
        })
    }

    fn is_maximized(&self) -> bool {
        self.window.is_maximized()
    }

    fn is_minimized(&self) -> Option<bool> {
        self.window.is_minimized()
    }

    fn fullscreen(&self) -> Option<FullscreenMode> {
        self.window.fullscreen().map(|mode| match mode {
            winit::window::Fullscreen::Exclusive(_) => FullscreenMode::Exclusive,
            winit::window::Fullscreen::Borderless(_) => FullscreenMode::Borderless,
        })
    }

    fn set_window_level(&self, level: WindowLevel) {
        self.window.set_window_level(chrome::level(level));
    }

    fn set_icon(&self, icon: Option<&WindowIcon>) {
        self.window.set_window_icon(icon.and_then(chrome::icon));
    }

    fn set_theme(&self, theme: Option<ColorScheme>) {
        self.window.set_theme(theme.map(crate::theme::theme));
    }

    fn focus(&self) {
        self.window.focus_window();
    }

    fn request_attention(&self, urgent: bool) {
        // Critical rather than informational, because the only caller is an application saying
        // something needs the user now; a desktop that draws only one kind of attention draws that.
        self.window
            .request_user_attention(urgent.then_some(winit::window::UserAttentionType::Critical));
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

    /// Publishes through the adapter this window's loop holds for it.
    ///
    /// Nothing is published, and nothing is built, from any other thread: the adapters belong to
    /// the thread the loop runs on, and a caller elsewhere finds none. That is the same answer a
    /// machine with no assistive technology gives, and the frame's publishing phase — the only
    /// caller — runs on the loop's thread anyway.
    fn push_a11y_update(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        a11y::publish(self.id, build);
    }

    fn gpu(&self) -> Option<&dyn GpuSurface> {
        Some(self)
    }

    fn gpu_shared(self: Arc<Self>) -> Option<Arc<dyn GpuSurface>> {
        Some(self)
    }
}
