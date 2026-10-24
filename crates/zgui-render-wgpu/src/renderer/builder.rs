//! Opening a device, and refusing to guess when none of them works.

use std::ffi::CStr;
use std::sync::Arc;

use zgui_geom::{Device, Size};
use zgui_render::{GpuUnavailable, RenderTarget};

use crate::gpu::adapter;
use crate::gpu::device::Gpu;
use crate::gpu::extensions;
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
/// window's handles, make a surface on each candidate's instance, then configure it.
pub struct Builder {
    /// One instance per backend tier, each created when the candidate loop reaches it.
    instances: adapter::Instances,
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
            instances: adapter::Instances::new(backends),
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

    /// The instance of the first backend tier.
    ///
    /// A surface made from it cannot present on a fallback tier. [`Builder::for_surface`] takes the
    /// window's handles and makes the surface on each candidate's own instance.
    pub fn instance(&self) -> &wgpu::Instance {
        self.instances.primary()
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
        self.open(target, origin, |gpu: &Arc<Gpu>| {
            Ok(Presentation::Offscreen(Offscreen::new(
                gpu,
                extent(target),
                format,
                mutable_texture_formats,
            )))
        })
    }

    /// Opens a device presenting to the window that `window` names.
    ///
    /// Each candidate gets a new surface, made from `window` on the candidate's instance. The
    /// surface is configured under a validation error scope, because configuring it is the only way
    /// to find out whether an adapter can actually present to it. Before calling this, a window
    /// integration should attach its compositor notification through
    /// [`Builder::with_pre_present`], so queued redraws can be paced without blocking the thread
    /// that handles window events.
    pub fn for_surface(
        self,
        target: RenderTarget,
        window: impl Into<wgpu::SurfaceTarget<'static>> + Clone,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let setup = SurfaceSetup {
            size: extent(target),
            opaque: target.opaque,
            pacing: self.present_pacing,
        };
        self.open(target, Origin::Surface, WindowSurface::new(window, setup))
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
    ///
    /// No Vulkan device extension is asked for here. A builder opens one device for one target and
    /// rebuilds through a builder after a loss, so a list stated on it would reach one caller
    /// only. The list belongs to
    /// [`SharedGraphics::with_extensions`](crate::SharedGraphics::with_extensions), where every
    /// device-opening path reads it, so a replacement device carries it too.
    fn open(
        self,
        target: RenderTarget,
        origin: Origin,
        present: impl Proof,
    ) -> Result<WgpuRenderer, GpuUnavailable> {
        let (gpu, presentation) = open_device(&self.instances, &[], present)?;
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
///
/// `extensions` decides the order candidates are tried in as well as what each one is asked for.
/// An adapter whose physical device has every name is tried before one that lacks them, inside its
/// own tier, and the order inside each group stands. Without that ordering the first adapter to
/// open at all becomes the device for the rest of the process, and an adapter lacking the names
/// still opens; a machine with one adapter that can hand out scanout buffers and one that cannot
/// would then settle on whichever [`adapter::sort_key`] preferred. Where no adapter has them,
/// every adapter is still tried and one of them opens. [`Gpu::open`] says what the list does to a
/// device once an adapter is chosen.
pub(crate) fn open_device(
    instances: &adapter::Instances,
    extensions: &[&'static CStr],
    mut present: impl Proof,
) -> Result<(Arc<Gpu>, Presentation), GpuUnavailable> {
    let mut rejections: Vec<(String, String)> = Vec::new();
    let mut enumerated = 0usize;
    // Adapters passed over for having the names and then being refused a device carrying them.
    // Offered again at the end, where nothing else opened at all.
    let mut deferred: Vec<(wgpu::Instance, wgpu::Adapter)> = Vec::new();
    // Whether a list was asked for at all. With none, every candidate is equal and the pairing
    // below leaves the preference order exactly as `adapter::candidates` produced it.
    let asked = !extensions.is_empty();
    let mut backends = wgpu::Backends::empty();
    for (tier, instance) in instances.iter() {
        backends |= tier;
        present.tier(instance, tier);
        // Paired with what each one can grant, asked before any device exists. The `&&`
        // short-circuits, so `granted_by` is never called where no list was asked for.
        let mut candidates: Vec<(bool, wgpu::Adapter)> = adapter::candidates(instance, tier)
            .into_iter()
            .map(|candidate| {
                (
                    asked && extensions::granted_by(&candidate, extensions),
                    candidate,
                )
            })
            .collect();
        enumerated += candidates.len();
        // Stable, so the existing preference survives inside each group.
        candidates.sort_by_key(|(grants, _)| !grants);
        for (grants, candidate) in candidates {
            let name = adapter::describe(&candidate.get_info());
            // Kept only where it is about to be judged on the list, so the ordinary path clones
            // nothing.
            let again = grants.then(|| (instance.clone(), candidate.clone()));
            let gpu = match Gpu::open(instance.clone(), candidate, extensions) {
                Ok(gpu) => Arc::new(gpu),
                Err(reason) => {
                    rejections.push((name, reason));
                    continue;
                }
            };
            if grants && gpu.vulkan_extensions().is_empty() {
                // Its physical device had every name and the driver refused a device carrying
                // them. Passed over before `present` runs, so no surface is consumed and the next
                // candidate can still be offered the one this candidate never saw.
                rejections.push((
                    name,
                    "the adapter has the Vulkan device extensions asked for and the driver \
                     refused a device carrying them"
                        .to_owned(),
                ));
                deferred.extend(again);
                continue;
            }
            match offer(&gpu, &mut present) {
                Ok(presentation) => {
                    tracing::info!(adapter = %gpu.describe(), "graphics device opened");
                    return Ok((gpu, presentation));
                }
                Err(reason) => rejections.push((name, reason)),
            }
        }
    }

    // Nothing opened, and some adapter was passed over above for granting the list and then being
    // refused a device carrying it. Those are offered again with the list no longer required. A
    // device that copies each frame through the processor still draws, and a machine with one
    // adapter and a driver that refuses the list would otherwise have no device at all.
    for (instance, candidate) in deferred {
        let name = adapter::describe(&candidate.get_info());
        let gpu = match Gpu::open(instance, candidate, &[]) {
            Ok(gpu) => Arc::new(gpu),
            Err(reason) => {
                rejections.push((name, reason));
                continue;
            }
        };
        match offer(&gpu, &mut present) {
            Ok(presentation) => {
                tracing::warn!(
                    adapter = %gpu.describe(),
                    "graphics device opened without the Vulkan device extensions that were asked \
                     for"
                );
                return Ok((gpu, presentation));
            }
            Err(reason) => rejections.push((name, reason)),
        }
    }

    if enumerated == 0 {
        tracing::warn!(backends = ?backends, "no graphics adapter was found");
    }
    Err(adapter::unavailable(rejections))
}

/// What the candidate loop proves each adapter against.
pub(crate) trait Proof {
    /// Prepares for the adapters of `tier`, before the loop lists them from `instance`.
    fn tier(&mut self, instance: &wgpu::Instance, tier: wgpu::Backends) {
        let _ = (instance, tier);
    }

    /// Produces something for `gpu` to present to, or the reason it cannot.
    fn present(&mut self, gpu: &Arc<Gpu>) -> Result<Presentation, String>;
}

impl<F: FnMut(&Arc<Gpu>) -> Result<Presentation, String>> Proof for F {
    fn present(&mut self, gpu: &Arc<Gpu>) -> Result<Presentation, String> {
        self(gpu)
    }
}

/// Proves adapters against a window, with a surface made on the instance of each tier.
///
/// The surface of a tier is made before the loop lists the tier's adapters, because wgpu's GL
/// backend needs its surface to exist before its adapters. A candidate takes the surface, and a
/// later candidate gets a new one, so a refusal consumes nothing the next candidate needs.
pub(crate) struct WindowSurface<W> {
    /// The window's handles.
    window: W,
    /// How the surface is configured.
    setup: SurfaceSetup,
    /// The surface made for the current tier, until a candidate takes it.
    made: Option<(wgpu::Backends, wgpu::Surface<'static>)>,
}

impl<W: Into<wgpu::SurfaceTarget<'static>> + Clone> WindowSurface<W> {
    /// Proves adapters against the window that `window` names.
    pub(crate) fn new(window: W, setup: SurfaceSetup) -> Self {
        Self {
            window,
            setup,
            made: None,
        }
    }
}

impl<W: Into<wgpu::SurfaceTarget<'static>> + Clone> Proof for WindowSurface<W> {
    fn tier(&mut self, instance: &wgpu::Instance, tier: wgpu::Backends) {
        // A failure here is reported by `present`, which tries again on the candidate's instance.
        self.made = instance
            .create_surface(self.window.clone())
            .ok()
            .map(|surface| (tier, surface));
    }

    fn present(&mut self, gpu: &Arc<Gpu>) -> Result<Presentation, String> {
        let backend = wgpu::Backends::from(gpu.adapter().get_info().backend);
        let surface = match self.made.take() {
            Some((tier, surface)) if tier.contains(backend) => surface,
            _ => gpu
                .instance()
                .create_surface(self.window.clone())
                .map_err(|error| error.to_string())?,
        };
        if surface.get_capabilities(gpu.adapter()).formats.is_empty() {
            return Err("the surface is not compatible with this adapter".to_owned());
        }
        Ok(Presentation::Surface(Box::new(ConfiguredSurface::new(
            gpu, surface, self.setup,
        ))))
    }
}

/// Offers `gpu` to `present`, and answers what came back or why nothing did.
///
/// Configuring a surface is the only way to find out whether an adapter can present to it, and a
/// failure there raises a validation error instead of returning one. So the attempt runs inside an
/// error scope, and both halves of the answer are read.
fn offer(gpu: &Arc<Gpu>, present: &mut impl Proof) -> Result<Presentation, String> {
    let scope = gpu.device().push_error_scope(wgpu::ErrorFilter::Validation);
    let presentation = present.present(gpu);
    let validation = futures::executor::block_on(scope.pop());
    match (presentation, validation) {
        (Ok(presentation), None) => Ok(presentation),
        (Ok(_), Some(error)) => Err(error.to_string()),
        (Err(reason), _) => Err(reason),
    }
}

/// The extent a target asks for, never smaller than one pixel.
fn extent(target: RenderTarget) -> Size<i32, Device> {
    Size::new(target.size.width.max(1), target.size.height.max(1))
}
