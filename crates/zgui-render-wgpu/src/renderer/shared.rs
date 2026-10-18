//! One graphics device behind every window in a process.
//!
//! A device is the expensive thing. Opening a second one for a second window costs another driver
//! connection, another queue, another set of compiled pipelines and another copy of every fixed
//! buffer the vector rasteriser allocates — which is measured in hundreds of megabytes. None of
//! that is per-window: what a window actually owns is a swap chain, a composed target sized to it,
//! and the buffers of its own frame.
//!
//! So this holds the instance and the device, and hands out a renderer per surface. The device is
//! opened by the *first* surface rather than at construction, because whether an adapter is usable
//! is only known against something to present to.
//!
//! # What is shared, and what is not
//!
//! Shared: the device, queue and adapter ([`Gpu`]), and the pipelines. Pipelines are keyed by
//! format as well as kind, so two windows on displays that negotiated different formats populate
//! more entries in one map rather than needing maps of their own. Sharing them also means one
//! on-disk pipeline cache with one writer, where a device per window had two racing to write the
//! same blob.
//!
//! Not shared: everything sized to a window — the swap chain, the composed target, the frame
//! buffers, the group pool, the scroll scratch — and the atlas textures. The atlas stays per
//! renderer because atlas keys are minted by a *window's* content cache and mean nothing outside
//! it: two windows numbering their tiles independently would collide in one sink, and one window's
//! eviction would blank another's glyphs. Sharing it needs a shared content cache above the
//! renderer, which is work for another day.

use std::cell::RefCell;
use std::ffi::CStr;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use zgui_geom::{Device, Size};
use zgui_render::{GpuUnavailable, RenderTarget};

use crate::gpu::adapter;
use crate::gpu::device::Gpu;
use crate::gpu::surface::{ConfiguredSurface, PresentPacing, SurfaceSetup};
use crate::pipeline::Pipelines;
use crate::renderer::builder::{Proof, WindowSurface, open_device};
use crate::renderer::{Origin, PrePresent, WgpuRenderer};
use crate::target::swapchain::{Offscreen, Presentation, Supplied};

/// Everything device-level that the renderers on one device share.
pub(crate) struct DeviceState {
    /// The device, queue and adapter.
    pub(crate) gpu: Arc<Gpu>,
    /// The pipelines, built on demand and keyed by kind and format.
    pub(crate) pipelines: Rc<RefCell<Pipelines>>,
}

impl DeviceState {
    /// The state for a device nothing has drawn on yet.
    pub(crate) fn new(gpu: Arc<Gpu>) -> Rc<Self> {
        let pipelines = Rc::new(RefCell::new(Pipelines::new(&gpu)));
        Rc::new(Self { gpu, pipelines })
    }
}

/// The graphics every window in one process draws through.
///
/// A cheap handle: clone it into whatever opens windows. Single-threaded, like every renderer in
/// this crate — a device is shared between windows on one thread, never between threads.
///
/// ```no_run
/// use zgui_render_wgpu::SharedGraphics;
///
/// // Made once, before any window exists: an instance needs no window.
/// let graphics = SharedGraphics::new();
/// // Each window then asks for its own renderer, and they all land on one device.
/// ```
#[derive(Clone)]
pub struct SharedGraphics(Rc<Shared>);

/// What a [`SharedGraphics`] holds.
struct Shared {
    /// One instance per backend tier, each created when the candidate loop reaches it.
    instances: adapter::Instances,
    /// Which backends may be enumerated.
    backends: wgpu::Backends,
    /// The Vulkan device extensions every device opened here asks for.
    ///
    /// Held on the graphics, because a device this opens after a loss has to enable what the dead
    /// one did. See [`SharedGraphics::with_extensions`].
    extensions: Vec<&'static CStr>,
    /// The device most windows draw on, opened by the first surface and replaced after a loss.
    primary: RefCell<Option<Rc<DeviceState>>>,
    /// Whether the primary device belongs to a host, which then is the only thing that opens one.
    ///
    /// See [`SharedGraphics::with_gpu`].
    adopted: bool,
    /// Devices opened for surfaces the primary adapter cannot present to.
    ///
    /// Weak, so that a cache never keeps a device alive: when the last window on a fallback adapter
    /// closes, the device it held goes with it.
    fallbacks: RefCell<Vec<Weak<DeviceState>>>,
}

impl Default for SharedGraphics {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedGraphics {
    /// Graphics over the backends the environment allows.
    pub fn new() -> Self {
        Self::with_backends(adapter::requested_backends())
    }

    /// Graphics over exactly `backends`.
    ///
    /// The empty set is meaningful: it is how a machine with no usable graphics device is
    /// reproduced on a machine that has one. No Vulkan device extension is asked for;
    /// [`SharedGraphics::with_extensions`] is the constructor that states some.
    pub fn with_backends(backends: wgpu::Backends) -> Self {
        Self::with_backends_and_extensions(backends, Vec::new())
    }

    /// Graphics over the backends the environment allows, asking every device for `extensions`.
    ///
    /// `extensions` names Vulkan device extensions, and they survive because they are stated here.
    /// A device extension can be enabled only while a device is created, and this opens devices
    /// from five places — the first surface, the first offscreen renderer,
    /// [`SharedGraphics::open_gpu`], the replacement built after a device is lost, and the
    /// separate device a surface the primary adapter cannot present to gets. All five read this
    /// one list, so a program whose buffers depend on an extension keeps it across a device loss,
    /// keeps it on a second monitor hung off another card, and cannot be handed an extension-free
    /// device by asking in the wrong order.
    ///
    /// The list is all-or-nothing and a machine may refuse it. [`Gpu::vulkan_extensions`] on the
    /// device that opened says whether it was enabled, and a caller reads that instead of
    /// assuming: a console backend falls back to copying through the processor.
    ///
    /// ```
    /// use zgui_render_wgpu::SharedGraphics;
    ///
    /// let graphics = SharedGraphics::with_extensions(vec![c"VK_EXT_image_drm_format_modifier"]);
    ///
    /// assert_eq!(graphics.extensions(), [c"VK_EXT_image_drm_format_modifier"]);
    /// ```
    pub fn with_extensions(extensions: Vec<&'static CStr>) -> Self {
        Self::with_backends_and_extensions(adapter::requested_backends(), extensions)
    }

    /// Graphics over exactly `backends`, asking every device for `extensions`.
    ///
    /// For a caller that has to state both. [`SharedGraphics::with_backends`] and
    /// [`SharedGraphics::with_extensions`] each state one.
    pub fn with_backends_and_extensions(
        backends: wgpu::Backends,
        extensions: Vec<&'static CStr>,
    ) -> Self {
        Self(Rc::new(Shared {
            instances: adapter::Instances::new(backends),
            backends,
            extensions,
            adopted: false,
            primary: RefCell::new(None),
            fallbacks: RefCell::new(Vec::new()),
        }))
    }

    /// Graphics on a device a host opened, and on no other device.
    ///
    /// `gpu` is usually one from [`Gpu::from_existing`]. It becomes the primary device at once, so
    /// [`SharedGraphics::gpu`] and [`SharedGraphics::open_gpu`] return it and
    /// [`SharedGraphics::renderer_supplied`] can be called immediately. The instance is the one the
    /// device came from, and the backends are the backend of its adapter.
    ///
    /// These graphics never open a device. Where every entry point would otherwise open one — the
    /// first surface on a lost device, a surface the adapter cannot present to, the replacement
    /// after a loss — they refuse with a [`GpuUnavailable`] that says so. After the host has
    /// recovered from a loss, it gives the new device with [`SharedGraphics::adopt`].
    ///
    /// ```no_run
    /// # use zgui_render_wgpu::wgpu;
    /// # fn host() -> (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue) { unimplemented!() }
    /// use zgui_render_wgpu::{Gpu, SharedGraphics};
    ///
    /// let (instance, adapter, device, queue) = host();
    /// let graphics = SharedGraphics::with_gpu(Gpu::from_existing(instance, adapter, device, queue));
    /// let gpu = graphics.open_gpu().expect("the adopted device is the primary one");
    /// assert!(gpu.is_adopted());
    /// ```
    pub fn with_gpu(gpu: Arc<Gpu>) -> Self {
        let backends = wgpu::Backends::from(gpu.adapter().get_info().backend);
        Self(Rc::new(Shared {
            instances: adapter::Instances::existing(gpu.instance().clone(), backends),
            backends,
            extensions: gpu.vulkan_extensions().to_vec(),
            adopted: true,
            primary: RefCell::new(Some(DeviceState::new(gpu))),
            fallbacks: RefCell::new(Vec::new()),
        }))
    }

    /// Makes `gpu` the primary device, in place of the current one.
    ///
    /// For a host that owns the device: after it has recovered from a loss, it adopts the new
    /// device here. A renderer on the old device changes to the new one the next time it recovers
    /// from the loss. A renderer that presents into supplied textures cannot recover, because its
    /// textures belong to the old device; its owner makes a new renderer with textures on the new
    /// device.
    ///
    /// Adopting the device that is already the primary one changes nothing: the pipelines built
    /// on it are kept.
    pub fn adopt(&self, gpu: Arc<Gpu>) {
        let mut primary = self.0.primary.borrow_mut();
        if primary
            .as_ref()
            .is_some_and(|state| Arc::ptr_eq(&state.gpu, &gpu))
        {
            return;
        }
        *primary = Some(DeviceState::new(gpu));
    }

    /// Returns `true` where these graphics draw only on a device a host gave them.
    ///
    /// See [`SharedGraphics::with_gpu`].
    pub fn is_adopted(&self) -> bool {
        self.0.adopted
    }

    /// The instance of the first backend tier.
    ///
    /// A surface made from it cannot present on a fallback tier.
    /// [`SharedGraphics::renderer_for_surface`] takes the window's handles and makes the surface
    /// on the instance of each device it tries.
    pub fn instance(&self) -> &wgpu::Instance {
        self.0.instances.primary()
    }

    /// Which backends this will enumerate.
    pub fn backends(&self) -> wgpu::Backends {
        self.0.backends
    }

    /// Returns the Vulkan device extensions every device opened here asks for.
    ///
    /// What was *asked for*. What a device got is [`Gpu::vulkan_extensions`] on the device itself.
    pub fn extensions(&self) -> &[&'static CStr] {
        &self.0.extensions
    }

    /// The shared device, once a surface has chosen one.
    ///
    /// `None` before the first window opens, because an adapter is chosen against something to
    /// present to and nothing has presented yet.
    pub fn gpu(&self) -> Option<Arc<Gpu>> {
        self.0
            .primary
            .borrow()
            .as_ref()
            .map(|state| Arc::clone(&state.gpu))
    }

    /// Opens the shared device, or answers the one that is already open.
    ///
    /// [`SharedGraphics::gpu`] answers only a device that is already open. This one reverses the
    /// order every other entry point here uses: a caller that presents into textures of its own
    /// has to create them on a device, so it needs the device before it can ask for a renderer.
    /// The candidate loop proves each adapter with a one-pixel texture, because no surface exists
    /// to prove one with.
    pub fn open_gpu(&self) -> Result<Arc<Gpu>, GpuUnavailable> {
        if let Some(state) = self.usable_primary() {
            return Ok(Arc::clone(&state.gpu));
        }
        Ok(Arc::clone(&self.open_primary()?.gpu))
    }

    /// A renderer for one more window, on the shared device where it can present there.
    ///
    /// `window` gives the window's handles. Each device that is tried gets a new surface, made from
    /// them on that device's instance. A window the primary adapter cannot present to — a second
    /// monitor on another card — gets a device of its own, and windows in that position share
    /// *that* device with each other.
    pub fn renderer_for_surface(
        &self,
        target: RenderTarget,
        window: impl Into<wgpu::SurfaceTarget<'static>> + Clone,
        pacing: PresentPacing,
        pre_present: Option<PrePresent>,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let setup = SurfaceSetup {
            size: extent(target),
            opaque: target.opaque,
            pacing,
        };

        // A device that has been lost is not a device to hand another window: the next frame on it
        // would do nothing but notice the loss again.
        if let Some(state) = self.usable_primary() {
            match present_on(&state, &window, setup) {
                Ok(presentation) => {
                    return Ok(self.assemble(
                        state,
                        presentation,
                        target,
                        pre_present,
                        Origin::Surface,
                    ));
                }
                Err(Rejected::Incompatible) => {
                    return self.on_another_device(target, &window, setup, pre_present);
                }
                Err(Rejected::Failed(reason)) => {
                    return Err(GpuUnavailable::new().rejected(state.gpu.describe(), reason));
                }
            }
        }

        // The first surface: no device has been opened yet, so this is what chooses one.
        let (gpu, presentation) = self.open_one(WindowSurface::new(window, setup))?;
        let state = DeviceState::new(gpu);
        *self.0.primary.borrow_mut() = Some(Rc::clone(&state));
        Ok(self.assemble(state, presentation, target, pre_present, Origin::Surface))
    }

    /// A renderer presenting to a texture, on the shared device.
    ///
    /// What a test wanting two renderers on one device uses, and what an application compositing
    /// into something other than a window uses. The format rules are the offscreen ones.
    pub fn renderer_offscreen(
        &self,
        target: RenderTarget,
        format: wgpu::TextureFormat,
        mutable_texture_formats: bool,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let origin = Origin::Offscreen {
            format,
            mutable_texture_formats,
        };
        let extent = extent(target);
        if let Some(state) = self.usable_primary() {
            let presentation = Presentation::Offscreen(Offscreen::new(
                &state.gpu,
                extent,
                format,
                mutable_texture_formats,
            ));
            return Ok(self.assemble(state, presentation, target, None, origin));
        }
        let (gpu, presentation) = self.open_one(|gpu: &Arc<Gpu>| {
            Ok(Presentation::Offscreen(Offscreen::new(
                gpu,
                extent,
                format,
                mutable_texture_formats,
            )))
        })?;
        let state = DeviceState::new(gpu);
        *self.0.primary.borrow_mut() = Some(Rc::clone(&state));
        Ok(self.assemble(state, presentation, target, None, origin))
    }

    /// A renderer presenting into textures the caller supplies, on the shared device.
    ///
    /// What a backend that owns the buffers a display controller scans out of uses: the frame is
    /// copied into the buffer the hardware reads, so nothing is read back to the processor and
    /// nothing is copied through it. The caller chooses which of them each frame goes to with
    /// [`WgpuRenderer::present_into`].
    ///
    /// # Order
    ///
    /// The textures have to be created on this device first, through
    /// [`SharedGraphics::open_gpu`]. Nothing here can open one for them: a texture belongs to the
    /// device that created it, so a device opened at this point would be the wrong one for
    /// everything handed in. wgpu states no device on a texture handle, so the order is the only
    /// thing that keeps the two together and this refuses rather than guessing.
    ///
    /// A caller whose textures need Vulkan device extensions states them once, on the graphics,
    /// with [`SharedGraphics::with_extensions`]. Every device-opening path here reads that one
    /// list, [`SharedGraphics::open_gpu`] included, so the device the textures are created on is
    /// the device this presents into and it carries them. A list stated per call could not do
    /// that: the primary is fixed for the rest of the process once it is open, so a caller
    /// following the order above would open an extension-free device, find that its dma-buf images
    /// will not import, and have no way back.
    ///
    /// # Errors
    ///
    /// Refuses where no device is open, where `target` and the textures state different extents,
    /// and where the textures cannot be presented to as one set — [`Supplied::unusable`] says what
    /// that means. Each refusal names what was wrong.
    pub fn renderer_supplied(
        &self,
        target: RenderTarget,
        textures: Vec<wgpu::Texture>,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let Some(state) = self.usable_primary() else {
            return Err(GpuUnavailable::new().rejected(
                "the shared device",
                "supplied textures come from a device, so one has to be open before a renderer can \
                 present into them: open it with SharedGraphics::open_gpu",
            ));
        };
        // Asked here as well as inside `Supplied::new`, so that the reason reaches the caller: a
        // set that is refused is a set the caller has to fix.
        if let Some(reason) = Supplied::unusable(&textures) {
            return Err(GpuUnavailable::new().rejected(state.gpu.describe(), reason));
        }
        let wanted = extent(target);
        let Some(supplied) = Supplied::new(textures) else {
            return Err(GpuUnavailable::new().rejected(
                state.gpu.describe(),
                "the supplied textures cannot be presented to as one set",
            ));
        };
        // The target and the textures are supplied separately and both describe the same screen.
        // Where they disagree the copy that ends a frame would cover the whole buffer while
        // reading a composed target of the other extent, which is a stretched frame nothing
        // reports.
        if supplied.size() != wanted {
            return Err(GpuUnavailable::new().rejected(
                state.gpu.describe(),
                format!(
                    "the target is {}×{} and the supplied textures are {}×{}; one screen has one \
                     extent",
                    wanted.width,
                    wanted.height,
                    supplied.size().width,
                    supplied.size().height
                ),
            ));
        }
        Ok(self.assemble(
            state,
            Presentation::Supplied(supplied),
            target,
            None,
            Origin::Supplied,
        ))
    }

    /// The device to rebuild on after `lost` died, opened once and then answered to everyone.
    ///
    /// With N windows on one device, every one of them notices the same loss on its own next frame.
    /// Only the first opens a replacement; the rest are handed it. Opening one per window would
    /// leave N devices where there was one, and the windows would stop sharing anything.
    pub(crate) fn replacement_for(
        &self,
        lost: &Arc<Gpu>,
    ) -> Result<Rc<DeviceState>, GpuUnavailable> {
        if let Some(state) = self.usable_primary()
            && !Arc::ptr_eq(&state.gpu, lost)
        {
            return Ok(state);
        }
        self.open_primary()
    }

    /// Opens the shared device with no surface, and records it as the primary.
    ///
    /// A one-pixel offscreen target is what the candidate loop proves an adapter with here.
    /// Recovery has nothing else to use, the window's own surface having died with the device, and
    /// neither has a caller that has yet to create anything.
    fn open_primary(&self) -> Result<Rc<DeviceState>, GpuUnavailable> {
        let (gpu, _) = self.open_one(|gpu: &Arc<Gpu>| {
            Ok(Presentation::Offscreen(Offscreen::new(
                gpu,
                Size::new(1, 1),
                wgpu::TextureFormat::Rgba8UnormSrgb,
                false,
            )))
        })?;
        let state = DeviceState::new(gpu);
        // Assigning drops the strong reference to any dead device, so its memory is released as
        // soon as the last renderer has swapped over.
        *self.0.primary.borrow_mut() = Some(Rc::clone(&state));
        Ok(state)
    }

    /// Runs the candidate loop with everything this graphics was constructed to state.
    ///
    /// Every device opened here goes through this, which keeps the five entry points agreeing. The
    /// Vulkan device extensions matter most: a replacement device that read a different list would
    /// leave a program running on a device its buffers cannot be imported into, and there is no
    /// later point at which that could be noticed.
    fn open_one(&self, present: impl Proof) -> Result<(Arc<Gpu>, Presentation), GpuUnavailable> {
        if self.0.adopted {
            // The host owns the device. A device opened here would be one the host's resources
            // cannot be used with, and one the host does not know it has to keep alive.
            return Err(GpuUnavailable::new().rejected(
                "the adopted device",
                "these graphics draw only on the device the host gave them, and that device is \
                 lost or cannot present here: the host gives a new one with SharedGraphics::adopt",
            ));
        }
        open_device(&self.0.instances, &self.0.extensions, present)
    }

    /// The primary device, if there is one and it is still alive.
    fn usable_primary(&self) -> Option<Rc<DeviceState>> {
        let primary = self.0.primary.borrow();
        primary
            .as_ref()
            .filter(|state| !state.gpu.loss().is_lost())
            .map(Rc::clone)
    }

    /// A renderer for a window the primary adapter cannot present to.
    ///
    /// Windows in this position — a second monitor on another card — share a device with each other
    /// rather than each opening one, which is the same rule the primary follows.
    fn on_another_device(
        &self,
        target: RenderTarget,
        window: &(impl Into<wgpu::SurfaceTarget<'static>> + Clone),
        setup: SurfaceSetup,
        pre_present: Option<PrePresent>,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        self.0.fallbacks.borrow_mut().retain(|state| {
            state
                .upgrade()
                .is_some_and(|state| !state.gpu.loss().is_lost())
        });
        let known: Vec<Rc<DeviceState>> = self
            .0
            .fallbacks
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        for state in known {
            match present_on(&state, window, setup) {
                Ok(presentation) => {
                    return Ok(self.assemble(
                        state,
                        presentation,
                        target,
                        pre_present,
                        Origin::Surface,
                    ));
                }
                Err(Rejected::Incompatible) => {}
                Err(Rejected::Failed(reason)) => {
                    return Err(GpuUnavailable::new().rejected(state.gpu.describe(), reason));
                }
            }
        }

        let (gpu, presentation) = self.open_one(WindowSurface::new(window.clone(), setup))?;
        let state = DeviceState::new(gpu);
        self.0.fallbacks.borrow_mut().push(Rc::downgrade(&state));
        Ok(self.assemble(state, presentation, target, pre_present, Origin::Surface))
    }

    /// Builds the renderer, telling it which graphics it belongs to so it can recover with them.
    fn assemble(
        &self,
        state: Rc<DeviceState>,
        presentation: Presentation,
        target: RenderTarget,
        pre_present: Option<PrePresent>,
        origin: Origin,
    ) -> WgpuRenderer {
        WgpuRenderer::assemble(
            state,
            Some(self.clone()),
            presentation,
            target,
            pre_present,
            origin,
            self.0.backends,
        )
    }
}

/// Configures a new surface for `window` on an already-open device, saying which kind of failure
/// it was.
fn present_on(
    state: &Rc<DeviceState>,
    window: &(impl Into<wgpu::SurfaceTarget<'static>> + Clone),
    setup: SurfaceSetup,
) -> Result<Presentation, Rejected> {
    let surface = state
        .gpu
        .instance()
        .create_surface(window.clone())
        .map_err(|_| Rejected::Incompatible)?;
    if surface
        .get_capabilities(state.gpu.adapter())
        .formats
        .is_empty()
    {
        return Err(Rejected::Incompatible);
    }
    let scope = state
        .gpu
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let presentation =
        Presentation::Surface(Box::new(ConfiguredSurface::new(&state.gpu, surface, setup)));
    match futures::executor::block_on(scope.pop()) {
        None => Ok(presentation),
        Some(error) => Err(Rejected::Failed(error.to_string())),
    }
}

/// Why a device would not take a window.
enum Rejected {
    /// The adapter cannot present to it, and another adapter can try.
    Incompatible,
    /// The adapter accepted it and then failed to configure it.
    Failed(String),
}

/// The extent a target asks for, never smaller than one pixel.
fn extent(target: RenderTarget) -> Size<i32, Device> {
    Size::new(target.size.width.max(1), target.size.height.max(1))
}
