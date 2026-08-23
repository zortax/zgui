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
use std::rc::{Rc, Weak};
use std::sync::Arc;

use zgui_geom::{Device, Size};
use zgui_render::{GpuUnavailable, RenderTarget};

use crate::gpu::adapter;
use crate::gpu::device::Gpu;
use crate::gpu::surface::{ConfiguredSurface, PresentPacing, SurfaceSetup};
use crate::pipeline::Pipelines;
use crate::renderer::builder::open_device;
use crate::renderer::{Origin, PrePresent, WgpuRenderer};
use crate::target::swapchain::{Offscreen, Presentation};

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
    /// The instance every surface is created from.
    instance: wgpu::Instance,
    /// Which backends may be enumerated.
    backends: wgpu::Backends,
    /// The device most windows draw on, opened by the first surface and replaced after a loss.
    primary: RefCell<Option<Rc<DeviceState>>>,
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
    /// reproduced on a machine that has one.
    pub fn with_backends(backends: wgpu::Backends) -> Self {
        Self(Rc::new(Shared {
            instance: wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            }),
            backends,
            primary: RefCell::new(None),
            fallbacks: RefCell::new(Vec::new()),
        }))
    }

    /// The instance, so that whatever owns a native window can create a surface from it.
    pub fn instance(&self) -> &wgpu::Instance {
        &self.0.instance
    }

    /// Which backends this will enumerate.
    pub fn backends(&self) -> wgpu::Backends {
        self.0.backends
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

    /// A renderer for one more window, on the shared device where it can present there.
    ///
    /// The surface must have been created from [`SharedGraphics::instance`]. A surface the primary
    /// adapter cannot present to — a second monitor on another card — gets a device of its own, and
    /// windows in that position share *that* device with each other.
    pub fn renderer_for_surface(
        &self,
        target: RenderTarget,
        surface: wgpu::Surface<'static>,
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
            match self.present_on(&state, surface, setup) {
                Ok(presentation) => {
                    return Ok(self.assemble(
                        state,
                        presentation,
                        target,
                        pre_present,
                        Origin::Surface,
                    ));
                }
                Err(Rejected::Incompatible(surface)) => {
                    return self.on_another_device(target, surface, setup, pre_present);
                }
                Err(Rejected::Failed(reason)) => {
                    // The surface was consumed proving the adapter cannot use it, so there is
                    // nothing left to try another adapter with. Whoever owns the window makes
                    // another surface and asks again — the same contract the candidate loop has.
                    return Err(GpuUnavailable::new().rejected(state.gpu.describe(), reason));
                }
            }
        }

        // The first surface: no device has been opened yet, so this is what chooses one.
        let mut surface = Some(surface);
        let (gpu, presentation) = open_device(&self.0.instance, self.0.backends, |gpu| {
            let surface = surface
                // A surface is configured for the adapter that accepted it, and a configured
                // surface cannot be handed to a second one. When the first candidate fails, the
                // window has to produce another surface, which is a decision for whoever owns it.
                .take()
                .ok_or_else(|| "the surface was consumed by an earlier candidate".to_owned())?;
            if surface.get_capabilities(gpu.adapter()).formats.is_empty() {
                return Err("the surface is not compatible with this adapter".to_owned());
            }
            Ok(Presentation::Surface(Box::new(ConfiguredSurface::new(
                gpu, surface, setup,
            ))))
        })?;
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
        let (gpu, presentation) = open_device(&self.0.instance, self.0.backends, |gpu| {
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
        // No surface exists during recovery — the window's own surface died with the device — so a
        // small offscreen target is what the candidate loop proves an adapter with.
        let (gpu, _) = open_device(&self.0.instance, self.0.backends, |gpu| {
            Ok(Presentation::Offscreen(Offscreen::new(
                gpu,
                Size::new(1, 1),
                wgpu::TextureFormat::Rgba8UnormSrgb,
                false,
            )))
        })?;
        let state = DeviceState::new(gpu);
        // Assigning drops the strong reference to the dead device, so its memory is released as
        // soon as the last renderer has swapped over.
        *self.0.primary.borrow_mut() = Some(Rc::clone(&state));
        Ok(state)
    }

    /// The primary device, if there is one and it is still alive.
    fn usable_primary(&self) -> Option<Rc<DeviceState>> {
        let primary = self.0.primary.borrow();
        primary
            .as_ref()
            .filter(|state| !state.gpu.loss().is_lost())
            .map(Rc::clone)
    }

    /// Configures `surface` for an already-open device, saying which kind of failure it was.
    ///
    /// Compatibility is asked before anything is consumed, so a surface the adapter cannot use
    /// comes back intact and can be offered to another one.
    fn present_on(
        &self,
        state: &Rc<DeviceState>,
        surface: wgpu::Surface<'static>,
        setup: SurfaceSetup,
    ) -> Result<Presentation, Rejected> {
        if surface
            .get_capabilities(state.gpu.adapter())
            .formats
            .is_empty()
        {
            return Err(Rejected::Incompatible(surface));
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

    /// A renderer for a surface the primary adapter cannot present to.
    ///
    /// Windows in this position — a second monitor on another card — share a device with each other
    /// rather than each opening one, which is the same rule the primary follows.
    fn on_another_device(
        &self,
        target: RenderTarget,
        surface: wgpu::Surface<'static>,
        setup: SurfaceSetup,
        pre_present: Option<PrePresent>,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let mut surface = Some(surface);

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
            let Some(offered) = surface.take() else { break };
            match self.present_on(&state, offered, setup) {
                Ok(presentation) => {
                    return Ok(self.assemble(
                        state,
                        presentation,
                        target,
                        pre_present,
                        Origin::Surface,
                    ));
                }
                Err(Rejected::Incompatible(returned)) => surface = Some(returned),
                Err(Rejected::Failed(reason)) => {
                    return Err(GpuUnavailable::new().rejected(state.gpu.describe(), reason));
                }
            }
        }

        let Some(surface) = surface else {
            return Err(GpuUnavailable::new().rejected(
                "every known device",
                "the surface was consumed by an earlier candidate",
            ));
        };
        let mut surface = Some(surface);
        let (gpu, presentation) = open_device(&self.0.instance, self.0.backends, |gpu| {
            let surface = surface
                .take()
                .ok_or_else(|| "the surface was consumed by an earlier candidate".to_owned())?;
            if surface.get_capabilities(gpu.adapter()).formats.is_empty() {
                return Err("the surface is not compatible with this adapter".to_owned());
            }
            Ok(Presentation::Surface(Box::new(ConfiguredSurface::new(
                gpu, surface, setup,
            ))))
        })?;
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

/// Why a device would not take a surface.
enum Rejected {
    /// The adapter cannot present to it. The surface comes back untouched, for another adapter.
    Incompatible(wgpu::Surface<'static>),
    /// The adapter accepted it and then failed, consuming it in the process.
    Failed(String),
}

/// The extent a target asks for, never smaller than one pixel.
fn extent(target: RenderTarget) -> Size<i32, Device> {
    Size::new(target.size.width.max(1), target.size.height.max(1))
}
