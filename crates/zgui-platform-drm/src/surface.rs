//! One output, seen as a thing the framework draws into.

use std::os::fd::{AsFd, AsRawFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use accesskit::TreeUpdate;
use raw_window_handle::{
    DisplayHandle, DrmDisplayHandle, DrmWindowHandle, HandleError, HasDisplayHandle,
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle,
};
use zgui_drm::Device as DrmDevice;
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
/// # The handles it reports
///
/// A KMS display has native handles and this reports them: the device descriptor as
/// `DrmDisplayHandle`, and the primary plane as `DrmWindowHandle`. [`Surface::gpu`] and
/// [`Surface::gpu_shared`] are the only route to them — a consumer holds `Arc<dyn Surface>` and
/// the contract offers nothing else — so a DRM-aware renderer reaches this backend's native state
/// only because both answer.
///
/// The device is held through an [`Arc`]. That is what a handed-out handle rests on: the descriptor
/// stays open, and the plane keeps naming a live object, for as long as anything holds the surface.
#[derive(Debug)]
pub struct DrmSurface {
    /// Which surface this is, in the contract's numbering.
    id: SurfaceId,
    /// The display this draws to: the pipe a commit names, and the mode it runs at.
    output: Output,
    /// The device the display hangs off, kept open for as long as this surface lives.
    ///
    /// Shared rather than owned: the frame loop drives the same device — [`Output::discover`],
    /// a commit and a poll all take `&Device` — and one device serves every display it lights, so
    /// the surfaces and the loop each hold a count on it. [`zgui_drm::Device`] is not [`Clone`],
    /// and it must not become so: a second owner would be a second close of the same descriptor.
    device: Arc<DrmDevice>,
    /// Whether a frame has been asked for and not yet taken.
    redraw: AtomicBool,
}

impl DrmSurface {
    /// Creates a surface over `output` on `device`, numbered `id`, with no frame asked for yet.
    ///
    /// `output` must be one this `device` enumerated. The handles this surface reports pair the
    /// device's descriptor with the output's plane, and a plane from a different device names
    /// either nothing or the wrong object.
    pub fn new(id: SurfaceId, output: Output, device: Arc<DrmDevice>) -> Self {
        Self {
            id,
            output,
            device,
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

    /// Returns `true` if a frame has been asked for and not yet taken, leaving the request where it
    /// is.
    ///
    /// The frame loop reads this after it has asked the application how to wait. A request on a
    /// console moves no descriptor — it is this flag and nothing else — so a loop that parked
    /// without looking would sleep through a frame somebody is waiting for. The ordinary case is a
    /// deadline the application turns into a redraw from inside the callback that reports it.
    pub fn wants_redraw(&self) -> bool {
        self.redraw.load(Ordering::Relaxed)
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

    /// Returns this surface, with the DRM handles it carries.
    ///
    /// A consumer holds `Arc<dyn Surface>` and the contract offers no other way down to a backend's
    /// native state, so this answer decides whether the handles are reachable at all. They are
    /// real, so it answers with them.
    ///
    /// No graphics API in this workspace's dependency set reads the two DRM variants yet. wgpu
    /// refuses a DRM window handle as "not a Vulkan-compatible handle", which is a true report of
    /// where the gap is. `App::run_drm` replaces the renderer factory with one that draws through
    /// this backend.
    fn gpu(&self) -> Option<&dyn GpuSurface> {
        Some(self)
    }

    fn gpu_shared(self: Arc<Self>) -> Option<Arc<dyn GpuSurface>> {
        Some(self)
    }
}

/// The device this display is driven through.
///
/// `DrmDisplayHandle` carries the descriptor as a raw number, so what keeps it valid is the
/// [`Arc`] the surface holds: a graphics API given a shared handle to the surface holds the device
/// open for as long as it draws.
impl HasDisplayHandle for DrmSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let handle = DrmDisplayHandle::new(self.device.as_fd().as_raw_fd());
        // SAFETY: the surface owns a count on the device, so the descriptor stays open for at
        // least as long as this borrow of the surface, which is the lifetime the handle carries.
        Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::Drm(handle)) })
    }
}

/// The primary plane this display scans out from.
///
/// It is the plane [`Output::discover`] chose for this output, which is the plane
/// `DrmWindowHandle::plane` is defined to name.
impl HasWindowHandle for DrmSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = DrmWindowHandle::new(self.output.pipe.plane);
        // SAFETY: the plane id came from this same device's own enumeration, and the surface owns
        // a count on that device, so the id names a live object for at least as long as this
        // borrow of the surface, which is the lifetime the handle carries.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Drm(handle)) })
    }
}

/// Returns one surface for each display `device` drives, in the order the displays were found.
///
/// The conversion lives here because it is a statement about a [`DrmSurface`]: how a surface is
/// numbered, and which device it holds open.
///
/// The numbers start at one and are never reused. A display is never destroyed, so nothing frees a
/// number for a second surface to take.
///
/// `outputs` must be the ones this `device` enumerated. The handles a surface reports pair the
/// device's descriptor with the output's plane, and a plane from a different device names either
/// nothing or the wrong object.
///
/// The frame loop calls this. It discovers the outputs, builds the surfaces here, describes them
/// with [`output::describe`](crate::output::describe), and hands both to
/// [`DrmCx::new`](crate::cx::DrmCx::new).
///
/// ```no_run
/// use std::sync::Arc;
/// use zgui_drm::Device;
/// use zgui_platform::{Surface, SurfaceId};
/// use zgui_platform_drm::Output;
/// use zgui_platform_drm::surface::one_per_output;
///
/// let device = Arc::new(Device::open_first().expect("a card on this machine"));
/// let outputs = Output::discover(&device).expect("the device describes itself");
/// let surfaces = one_per_output(outputs, device);
///
/// for (place, surface) in surfaces.iter().enumerate() {
///     assert_eq!(
///         surface.id(),
///         SurfaceId::new(place as u64 + 1),
///         "the numbers start at one and follow the order the displays were found"
///     );
/// }
/// ```
pub fn one_per_output(outputs: Vec<Output>, device: Arc<DrmDevice>) -> Vec<Arc<DrmSurface>> {
    (1..)
        .zip(outputs)
        .map(|(id, output)| {
            Arc::new(DrmSurface::new(
                SurfaceId::new(id),
                output,
                Arc::clone(&device),
            ))
        })
        .collect()
}
