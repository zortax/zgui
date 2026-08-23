//! Opening a device, and refusing to guess when none of them works.

use std::sync::Arc;

use zgui_geom::{Device, Size};
use zgui_render::{GpuUnavailable, RenderTarget};

use crate::gpu::adapter;
use crate::gpu::device::Gpu;
use crate::gpu::surface::{ConfiguredSurface, PresentPacing, SurfaceSetup};
use crate::renderer::shared::DeviceState;
use crate::renderer::{Origin, PrePresent, WgpuRenderer};
use crate::target::swapchain::{Offscreen, Presentation};

/// Builds a renderer: an instance first, then a device, then something to present to.
///
/// The order matters and is why this is a builder rather than a constructor. A surface has to be
/// created from the same instance the device came from, and whether an adapter is *usable* is only
/// known once a device has been created from it and a surface configured for it — adapters on
/// hybrid graphics report capabilities a device made from them turns out not to have. So: take the
/// instance, let whatever owns a window make a surface from it, then run the candidate loop.
pub struct Builder {
    /// The instance surfaces are created from.
    instance: wgpu::Instance,
    /// Which backends were enumerated.
    backends: wgpu::Backends,
    /// What to run between submitting a frame and presenting it.
    pre_present: Option<PrePresent>,
    /// Who waits for the display.
    present_pacing: PresentPacing,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    /// An instance over the backends the environment allows.
    pub fn new() -> Self {
        Self::with_backends(adapter::requested_backends())
    }

    /// An instance over exactly `backends`.
    ///
    /// The empty set is meaningful and is not a mistake: it is how a machine with no usable
    /// graphics device is reproduced on a machine that has one, which is the only way to exercise
    /// what such a user sees without uninstalling a driver.
    pub fn with_backends(backends: wgpu::Backends) -> Self {
        Self {
            instance: wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            }),
            backends,
            pre_present: None,
            present_pacing: PresentPacing::Display,
        }
    }

    /// Configures presentation for `pacing` rather than for the display.
    ///
    /// A window integration that paces frames against its own compositor timing asks for
    /// [`PresentPacing::Platform`] here, and must then actually pace them: nothing below this line
    /// will wait for the display on its behalf.
    pub fn with_present_pacing(mut self, pacing: PresentPacing) -> Self {
        self.present_pacing = pacing;
        self
    }

    /// The instance, so that whatever owns a native window can create a surface from it.
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// Which backends this will enumerate.
    pub fn backends(&self) -> wgpu::Backends {
        self.backends
    }

    /// Runs `notify` between submitting a frame's work and presenting it.
    ///
    /// That is the one point in a frame where telling a compositor that one is coming is
    /// meaningful: after the work exists, before it is handed over. A window-backed renderer
    /// should connect this to the notification supplied by whatever owns the native window. The
    /// callback runs only when the acquisition produced a frame that will be presented; a failed
    /// acquisition never leaves the compositor waiting for a commit that will not arrive.
    pub fn with_pre_present(mut self, notify: PrePresent) -> Self {
        self.pre_present = Some(notify);
        self
    }

    /// Opens a device presenting to a texture rather than to a window.
    ///
    /// `format` and `mutable_texture_formats` are what a surface would have offered and what a
    /// device would have reported, so the format rules — including both fallbacks for a surface
    /// that offers only an encoded format — are exercised exactly as they would be on a window.
    pub fn offscreen(
        self,
        target: RenderTarget,
        format: wgpu::TextureFormat,
        mutable_texture_formats: bool,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let origin = Origin::Offscreen {
            format,
            mutable_texture_formats,
        };
        self.open(target, origin, |gpu| {
            Ok(Presentation::Offscreen(Offscreen::new(
                gpu,
                extent(target),
                format,
                mutable_texture_formats,
            )))
        })
    }

    /// Opens a device presenting to a window's `surface`.
    ///
    /// The surface must have been created from [`Builder::instance`]. It is configured here, under
    /// a validation error scope, because configuring it is the only way to find out whether an
    /// adapter can actually present to it. Before calling this, a window integration should attach
    /// its compositor notification through [`Builder::with_pre_present`], so queued redraws can be
    /// paced without blocking the thread that handles window events.
    pub fn for_surface(
        self,
        target: RenderTarget,
        surface: wgpu::Surface<'static>,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let mut surface = Some(surface);
        let setup = SurfaceSetup {
            size: extent(target),
            opaque: target.opaque,
            pacing: self.present_pacing,
        };
        self.open(target, Origin::Surface, move |gpu| {
            let surface = surface
                .take()
                // A surface is configured for the adapter that accepted it, and a configured
                // surface cannot be handed to a second one. When the first candidate fails, the
                // window has to produce another surface, which is a decision for whoever owns it.
                .ok_or_else(|| "the surface was consumed by an earlier candidate".to_owned())?;
            if surface.get_capabilities(gpu.adapter()).formats.is_empty() {
                return Err("the surface is not compatible with this adapter".to_owned());
            }
            Ok(Presentation::Surface(Box::new(ConfiguredSurface::new(
                gpu, surface, setup,
            ))))
        })
    }

    /// Tries every candidate adapter in preference order, one backend tier at a time.
    ///
    /// Each candidate is accepted only once a device has been created from it *and* `present` has
    /// produced something to present to without raising a validation error. When none survives,
    /// the failure names every adapter tried and why — silently rendering somewhere the user
    /// cannot see is worse than not starting, because a window that appears and never paints looks
    /// like a program that has hung.
    ///
    /// A tier is enumerated only when every tier before it has failed, so the backends kept as a
    /// fallback cost a machine that never needs them nothing at all.
    fn open(
        self,
        target: RenderTarget,
        origin: Origin,
        present: impl FnMut(&Arc<Gpu>) -> Result<Presentation, String>,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let (gpu, presentation) = open_device(&self.instance, self.backends, present)?;
        Ok(WgpuRenderer::assemble(
            DeviceState::new(gpu),
            None,
            presentation,
            target,
            self.pre_present,
            origin,
            self.backends,
        ))
    }
}

/// Tries every candidate adapter in preference order, one backend tier at a time.
///
/// Each candidate is accepted only once a device has been created from it *and* `present` has
/// produced something to present to without raising a validation error. When none survives, the
/// failure names every adapter tried and why — silently rendering somewhere the user cannot see is
/// worse than not starting, because a window that appears and never paints looks like a program
/// that has hung.
///
/// A tier is enumerated only when every tier before it has failed, so the backends kept as a
/// fallback cost a machine that never needs them nothing at all.
///
/// Separate from [`Builder`] because opening a device and assembling a renderer are two things:
/// [`SharedGraphics`](crate::SharedGraphics) opens one device and assembles many renderers on it.
pub(crate) fn open_device(
    instance: &wgpu::Instance,
    backends: wgpu::Backends,
    mut present: impl FnMut(&Arc<Gpu>) -> Result<Presentation, String>,
) -> Result<(Arc<Gpu>, Presentation), GpuUnavailable> {
    let mut rejections: Vec<(String, String)> = Vec::new();
    let mut enumerated = 0usize;
    for tier in adapter::tiers(backends) {
        let candidates = adapter::candidates(instance, tier);
        enumerated += candidates.len();
        for candidate in candidates {
            let name = adapter::describe(&candidate.get_info());
            let gpu = match Gpu::open(instance.clone(), candidate) {
                Ok(gpu) => Arc::new(gpu),
                Err(reason) => {
                    rejections.push((name, reason));
                    continue;
                }
            };
            let scope = gpu.device().push_error_scope(wgpu::ErrorFilter::Validation);
            let presentation = present(&gpu);
            let validation = futures::executor::block_on(scope.pop());
            match (presentation, validation) {
                (Ok(presentation), None) => {
                    tracing::info!(adapter = %gpu.describe(), "graphics device opened");
                    return Ok((gpu, presentation));
                }
                (Ok(_), Some(error)) => rejections.push((name, error.to_string())),
                (Err(reason), _) => rejections.push((name, reason)),
            }
        }
    }
    if enumerated == 0 {
        tracing::warn!(backends = ?backends, "no graphics adapter was found");
    }
    Err(adapter::unavailable(rejections))
}

/// The extent a target asks for, never smaller than one pixel.
fn extent(target: RenderTarget) -> Size<i32, Device> {
    Size::new(target.size.width.max(1), target.size.height.max(1))
}
