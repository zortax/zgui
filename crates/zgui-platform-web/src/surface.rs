//! The canvas as a surface.
//!
//! A [`zgui_platform::Surface`] is `Send + Sync`, and a browser object is neither. So the surface
//! holds numbers only: its identifier, its size, its scale and whether it waits for a frame. What
//! it asks of the page, it asks through the driver, which holds the canvas on the page's thread.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use accesskit::TreeUpdate;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::{
    CursorStyle, Decorations, FullscreenMode, PresentationTiming, Surface, SurfaceId, TextInput,
};

use crate::driver;

/// One canvas of the page.
#[derive(Debug)]
pub struct CanvasSurface {
    /// The identifier.
    id: SurfaceId,
    /// The width of the drawing buffer, in device pixels.
    width: AtomicU32,
    /// The height of the drawing buffer, in device pixels.
    height: AtomicU32,
    /// The device pixel ratio, as the bits of an `f64`.
    scale: AtomicU64,
    /// Whether the application asked for a frame it has not been given yet.
    redraw: AtomicBool,
    /// The display's refresh rate in millihertz, once enough frames measured it. Zero before.
    refresh: AtomicU32,
}

impl CanvasSurface {
    /// A surface of `size` device pixels at `scale`.
    pub(crate) fn new(id: SurfaceId, size: (u32, u32), scale: f64) -> Self {
        Self {
            id,
            width: AtomicU32::new(size.0),
            height: AtomicU32::new(size.1),
            scale: AtomicU64::new(scale.to_bits()),
            redraw: AtomicBool::new(false),
            refresh: AtomicU32::new(0),
        }
    }

    /// Records a new size and scale, and returns whether the scale changed.
    pub(crate) fn resize(&self, size: (u32, u32), scale: f64) -> bool {
        self.width.store(size.0, Ordering::Relaxed);
        self.height.store(size.1, Ordering::Relaxed);
        self.scale.swap(scale.to_bits(), Ordering::Relaxed) != scale.to_bits()
    }

    /// The size of the drawing buffer, in device pixels.
    pub(crate) fn extent(&self) -> (u32, u32) {
        (
            self.width.load(Ordering::Relaxed),
            self.height.load(Ordering::Relaxed),
        )
    }

    /// Takes the request for a frame, if there is one.
    pub(crate) fn take_redraw(&self) -> bool {
        self.redraw.swap(false, Ordering::Relaxed)
    }

    /// Whether the application waits for a frame.
    pub(crate) fn wants_redraw(&self) -> bool {
        self.redraw.load(Ordering::Relaxed)
    }

    /// Records the refresh rate the animation frames measured.
    pub(crate) fn set_refresh(&self, millihertz: u32) {
        self.refresh.store(millihertz, Ordering::Relaxed);
    }
}

impl Surface for CanvasSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn size(&self) -> Size<DevicePx, Device> {
        let (width, height) = self.extent();
        Size::new(DevicePx(width as f32), DevicePx(height as f32))
    }

    fn scale_factor(&self) -> f64 {
        f64::from_bits(self.scale.load(Ordering::Relaxed))
    }

    fn refresh_rate_millihertz(&self) -> Option<u32> {
        match self.refresh.load(Ordering::Relaxed) {
            0 => None,
            rate => Some(rate),
        }
    }

    fn presentation_timing(&self) -> Option<PresentationTiming> {
        driver::with(|driver| driver.presentation_timing()).flatten()
    }

    fn request_size(&self, _size: Size<CssPx, Css>) -> Option<Size<DevicePx, Device>> {
        // The page sets the size of a canvas with CSS.
        None
    }

    fn set_min_size(&self, _size: Option<Size<CssPx, Css>>) {}

    fn set_max_size(&self, _size: Option<Size<CssPx, Css>>) {}

    fn request_redraw(&self) {
        self.redraw.store(true, Ordering::Relaxed);
        driver::with(|driver| driver.schedule_frame());
    }

    fn pre_present_notify(&self) {}

    fn set_title(&self, title: &str) {
        driver::with(|driver| driver.set_title(title));
    }

    fn set_visible(&self, visible: bool) {
        driver::with(|driver| driver.set_visible(self.id, visible));
    }

    fn set_decorations(&self, _decorations: Decorations) {}

    fn set_resizable(&self, _resizable: bool) {}

    fn set_maximized(&self, _maximized: bool) {}

    fn set_minimized(&self, _minimized: bool) {}

    fn set_fullscreen(&self, mode: Option<FullscreenMode>) {
        driver::with(|driver| driver.set_fullscreen(self.id, mode.is_some()));
    }

    fn fullscreen(&self) -> Option<FullscreenMode> {
        driver::with(|driver| driver.is_fullscreen(self.id))
            .unwrap_or(false)
            .then_some(FullscreenMode::Borderless)
    }

    fn focus(&self) {
        driver::with(|driver| driver.focus(self.id));
    }

    fn set_cursor(&self, cursor: CursorStyle) {
        driver::with(|driver| driver.set_cursor(self.id, cursor));
    }

    fn set_text_input(&self, state: Option<TextInput>) {
        driver::with(|driver| driver.set_text_input(self.id, state));
    }

    fn push_a11y_update(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        // The tree is built only when the page asked for it.
        driver::with(|driver| driver.update_a11y(self.id, build));
    }
}

/// The CSS name of a cursor.
pub(crate) const fn cursor_name(cursor: CursorStyle) -> &'static str {
    match cursor {
        CursorStyle::Pointer => "pointer",
        CursorStyle::Text => "text",
        CursorStyle::VerticalText => "vertical-text",
        CursorStyle::Crosshair => "crosshair",
        CursorStyle::Grab => "grab",
        CursorStyle::Grabbing => "grabbing",
        CursorStyle::Wait => "wait",
        CursorStyle::Progress => "progress",
        CursorStyle::NotAllowed => "not-allowed",
        CursorStyle::Move => "move",
        CursorStyle::ResizeColumn => "col-resize",
        CursorStyle::ResizeRow => "row-resize",
        CursorStyle::ResizeEastWest => "ew-resize",
        CursorStyle::ResizeNorthSouth => "ns-resize",
        CursorStyle::ResizeNorthEastSouthWest => "nesw-resize",
        CursorStyle::ResizeNorthWestSouthEast => "nwse-resize",
        CursorStyle::None => "none",
        _ => "default",
    }
}
