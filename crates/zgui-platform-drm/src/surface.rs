//! One output, seen as a thing the framework draws into.

use std::sync::atomic::{AtomicBool, Ordering};

use accesskit::TreeUpdate;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::{
    CursorStyle, Decorations, FullscreenMode, GpuSurface, Surface, SurfaceId, TextInput,
};

use crate::output::Output;

/// One display, seen as a surface.
///
/// A surface here is an output and an output is a fixed thing: it has the extent of its mode, it
/// covers the whole screen, and nothing manages it. So most of the contract answers by doing
/// nothing, and each of those answers says which absence it comes from — a missing window manager,
/// a missing compositor, or a part of this backend that is not written yet.
///
/// # No window handle
///
/// [`Surface::gpu`] answers nothing. A KMS surface is a plane and a framebuffer rather than a
/// window, so there is no handle for a graphics API to build a swap chain from, and the renderer
/// is supplied by the application through `App::with_renderer` instead. So this whole backend
/// leaves the platform contract unchanged.
#[derive(Debug)]
pub struct DrmSurface {
    /// Which surface this is, in the contract's numbering.
    id: SurfaceId,
    /// The display this draws to: the pipe a commit names, and the mode it runs at.
    output: Output,
    /// Whether a frame has been asked for and not yet taken.
    redraw: AtomicBool,
}

impl DrmSurface {
    /// A surface over `output`, numbered `id`, with no frame asked for yet.
    pub fn new(id: SurfaceId, output: Output) -> Self {
        Self {
            id,
            output,
            redraw: AtomicBool::new(false),
        }
    }

    /// Returns the display this draws to.
    ///
    /// The frame loop needs the pipe to commit a framebuffer, and the mode to allocate one.
    pub fn output(&self) -> &Output {
        &self.output
    }

    /// Takes the pending request, reporting whether there was one.
    ///
    /// Coalescing lives here: a hundred requests between two frames are one pending request and
    /// therefore one frame, which the contract promises. The frame loop calls it once per turn,
    /// for every output it drives.
    pub fn take_redraw(&self) -> bool {
        self.redraw.swap(false, Ordering::Relaxed)
    }
}

impl Surface for DrmSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    /// Returns the mode's extent, which the CRTC scans out.
    fn size(&self) -> Size<DevicePx, Device> {
        Size::new(
            DevicePx(self.output.mode.width() as f32),
            DevicePx(self.output.mode.height() as f32),
        )
    }

    /// Returns one. A console reports no scale, and an invented one would size every application
    /// differently on every machine.
    fn scale_factor(&self) -> f64 {
        1.0
    }

    /// Returns the mode's rate, which the kernel's timings give to the millihertz.
    fn refresh_rate_millihertz(&self) -> Option<u32> {
        // Zero is a mode whose timings give no rate at all, which is absent rather than infinitely
        // fast. The contract states the fallback once, and this reports the truth to it.
        Some(self.output.mode.refresh_rate_millihertz()).filter(|rate| *rate > 0)
    }

    /// Answers the current size unchanged: an output is the size of its mode.
    fn request_size(&self, _size: Size<CssPx, Css>) -> Option<Size<DevicePx, Device>> {
        Some(self.size())
    }

    /// Does nothing: an output is the size of its mode, so there is no smallest one to keep.
    fn set_min_size(&self, _size: Option<Size<CssPx, Css>>) {}

    /// Does nothing: an output is the size of its mode, so there is no largest one to keep.
    fn set_max_size(&self, _size: Option<Size<CssPx, Css>>) {}

    fn request_redraw(&self) {
        self.redraw.store(true, Ordering::Relaxed);
    }

    /// Does nothing: the compositor this would tell about a frame does not exist.
    fn pre_present_notify(&self) {}

    /// Does nothing: no window manager shows a title, so there is nowhere to put one.
    fn set_title(&self, _title: &str) {}

    /// Does nothing: an output shows whatever its plane holds and cannot be hidden.
    fn set_visible(&self, _visible: bool) {}

    /// Does nothing: no window manager draws a frame or a title bar.
    fn set_decorations(&self, _decorations: Decorations) {}

    /// Does nothing: no window manager offers a drag to resize with.
    fn set_resizable(&self, _resizable: bool) {}

    /// Does nothing: an output already fills the screen.
    fn set_maximized(&self, _maximized: bool) {}

    /// Does nothing: no window manager takes a surface off the screen.
    fn set_minimized(&self, _minimized: bool) {}

    /// Does nothing: an output takes the whole screen at its mode, which is full screen.
    fn set_fullscreen(&self, _mode: Option<FullscreenMode>) {}

    /// Answers with [`FullscreenMode::Exclusive`]. A mode set on a whole output is exactly that.
    fn fullscreen(&self) -> Option<FullscreenMode> {
        Some(FullscreenMode::Exclusive)
    }

    /// Nothing until the input sub-project: there is no pointer to give a shape to.
    fn set_cursor(&self, _cursor: CursorStyle) {}

    /// Does nothing until the input sub-project: there is no input method to steer.
    fn set_text_input(&self, _state: Option<TextInput>) {}

    /// Does nothing: no assistive technology is connected, so the closure is never run and no
    /// tree is built. This becomes a publication when there is a channel to push it to.
    fn push_a11y_update(&self, _build: &mut dyn FnMut() -> TreeUpdate) {}

    /// Returns nothing: there is no window handle to give.
    ///
    /// A KMS surface is a plane, a framebuffer and a page flip, so a graphics API cannot build a
    /// swap chain from it, and the application supplies its renderer through `App::with_renderer`
    /// instead. So this whole backend leaves the platform contract unchanged.
    fn gpu(&self) -> Option<&dyn GpuSurface> {
        None
    }
}
