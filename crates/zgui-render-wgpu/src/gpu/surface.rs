//! The surface configuration, and the only place `configure` is called.

use zgui_geom::{Device, Size};

use crate::gpu::device::Gpu;
use crate::gpu::formats::Formats;

/// A surface wait long enough to be visible as an application stall.
pub(crate) const HUMAN_VISIBLE_WAIT: std::time::Duration = std::time::Duration::from_millis(250);

/// Whether a surface operation took long enough to report as a stall.
pub(crate) fn human_visible_wait(elapsed: std::time::Duration) -> bool {
    elapsed >= HUMAN_VISIBLE_WAIT
}

/// How many frames the driver may have in flight before it blocks.
///
/// Two is one being composited and one being drawn. More adds latency between an input event and
/// the pixels answering it; fewer stalls the queue on every frame.
pub const FRAME_LATENCY: u32 = 2;

/// The presentation mode, pinned so the compositor paces the frame loop.
///
/// The event loop never spins: it waits on platform events and timer deadlines, and this is what
/// makes the compositor's own cadence the thing frames are paced against.
pub const PRESENT_MODE: wgpu::PresentMode = wgpu::PresentMode::Fifo;

/// A configured surface: the swap chain, the configuration it holds, and whether it is current.
///
/// Every field of the configuration is written here. Leaving one out is a decision by default, and
/// wgpu's defaults for the two that matter — the format and the alpha mode — are "whatever the
/// driver listed first" and "work it out later".
#[derive(Debug)]
pub struct ConfiguredSurface {
    /// The surface itself.
    surface: wgpu::Surface<'static>,
    /// The configuration it currently holds.
    config: wgpu::SurfaceConfiguration,
    /// The formats that configuration was derived from.
    formats: Formats,
    /// Whether the surface has been configured at least once and not invalidated since.
    configured: bool,
    /// Whether the surface asked to be reconfigured at the next opportunity.
    ///
    /// Set when acquisition reports the texture no longer matches the surface. The texture is
    /// still usable, so the frame presents; the reconfiguration happens before the next one.
    reconfigure_pending: bool,
    /// The extent asked for and not yet applied to the swap chain.
    ///
    /// A resize records its extent here and the swap chain is rebuilt later, in
    /// [`ConfiguredSurface::apply`]. See that method for why the two are not the same moment.
    pending: Option<Size<i32, Device>>,
}

impl ConfiguredSurface {
    /// Configures `surface` for `size`, picking its format and alpha mode from what it offers.
    ///
    /// # Panics
    ///
    /// Panics in a debug build when the chosen configuration would let the attachment encode what
    /// is drawn into it. That cannot happen through the format choice itself, which is exactly why
    /// the assertion is worth making: it is the guard on everything that might later be added
    /// beside it.
    pub fn new(
        gpu: &Gpu,
        surface: wgpu::Surface<'static>,
        size: Size<i32, Device>,
        opaque: bool,
    ) -> Self {
        let capabilities = surface.get_capabilities(gpu.adapter());
        tracing::info!(
            offered = ?capabilities.present_modes,
            chosen = ?PRESENT_MODE,
            frame_latency = FRAME_LATENCY,
            "surface presentation modes"
        );
        let formats = crate::gpu::formats::choose(
            &capabilities.formats,
            &capabilities.alpha_modes,
            opaque,
            gpu.capabilities().mutable_texture_formats,
        );
        formats.log(&gpu.describe());
        debug_assert!(
            formats.is_sound(),
            "an encoded surface format with nothing to cancel the encode: {formats:?}"
        );

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: formats.surface,
            width: size.width.max(1) as u32,
            height: size.height.max(1) as u32,
            present_mode: PRESENT_MODE,
            desired_maximum_frame_latency: FRAME_LATENCY,
            alpha_mode: formats.alpha_mode,
            view_formats: formats.view_formats(),
        };
        let mut configured = Self {
            surface,
            config,
            formats,
            configured: false,
            reconfigure_pending: false,
            pending: None,
        };
        configured.resize(size);
        configured.apply(gpu);
        configured
    }

    /// Records that the swap chain is wanted at `size`, without rebuilding it yet.
    ///
    /// A resize asks for this at the top of a frame, because everything sized against the surface
    /// — the composed target, the viewport a media query is matched against — is derived from what
    /// the window now is. Rebuilding the swap chain there as well would be an ordering nobody
    /// needs and the most expensive one available: see [`ConfiguredSurface::apply`].
    pub fn resize(&mut self, size: Size<i32, Device>) {
        let width = size.width.max(1) as u32;
        let height = size.height.max(1) as u32;
        if self.configured
            && !self.reconfigure_pending
            && (width, height) == (self.config.width, self.config.height)
        {
            // Rebuilding a swapchain is not a cheap call that happens to be redundant: it waits
            // for the device to go completely idle before it destroys the images, so it drains
            // every frame in flight. A quarter of the configures a drag produces repeat the
            // extent they were already at — the compositor sends the same size twice, and the
            // first frame after any redraw request asks again — and each of those paid the full
            // stall for a swapchain identical to the one it replaced.
            zgui_profile::latency::note("cfg.same", format!("{width}x{height}"));
            self.pending = None;
            return;
        }
        self.pending = Some(Size::new(width as i32, height as i32));
    }

    /// Rebuilds the swap chain if one is owed.
    ///
    /// Called once a frame, immediately before a texture is acquired, and that placement is the
    /// whole point. `wgpu::Surface::configure` destroys the old images, so it first waits for the
    /// device to go completely idle — every command buffer still in flight has to retire before it
    /// returns. At the top of a frame the previous frame's submission is at its youngest and that
    /// wait is a stall paid in full, ahead of any work that could have overlapped it. Here it is
    /// paid after the frame's whole CPU side — layout, emit, recording — has already given the
    /// device that time.
    ///
    /// # Panics
    ///
    /// Panics if a surface texture acquired from this surface is still alive, which wgpu forbids.
    /// Nothing can hold one across this call: acquisition and presentation both happen inside a
    /// single frame, and this runs before the acquisition.
    pub fn apply(&mut self, gpu: &Gpu) {
        let resized = self.pending.take();
        // A surface that has been configured for the extent it is at, and that nothing asked to
        // rebuild, owes nothing: an acquisition against it is the whole of the frame's dealing
        // with the swap chain.
        if resized.is_none() && self.configured && !self.reconfigure_pending {
            return;
        }
        if let Some(size) = resized {
            self.config.width = size.width.max(1) as u32;
            self.config.height = size.height.max(1) as u32;
        }
        zgui_profile::latency::mark("cfg.in");
        let began = std::time::Instant::now();
        self.surface.configure(gpu.device(), &self.config);
        let elapsed = began.elapsed();
        if human_visible_wait(elapsed) {
            tracing::warn!(
                stage = "configure",
                elapsed_ms = elapsed.as_millis() as u64,
                width = self.config.width,
                height = self.config.height,
                present_mode = ?self.config.present_mode,
                frame_latency = self.config.desired_maximum_frame_latency,
                "surface operation blocked the event-loop thread"
            );
        }
        zgui_profile::latency::note(
            "cfg.out",
            format!("{}x{}", self.config.width, self.config.height),
        );
        self.configured = true;
        self.reconfigure_pending = false;
    }

    /// Whether the surface has a current configuration.
    ///
    /// A frame drawn against an unconfigured surface records nothing and keeps its damage, rather
    /// than reaching wgpu's own panic on the way to acquiring a texture.
    pub fn is_configured(&self) -> bool {
        self.configured
    }

    /// Marks the configuration as no longer matching the window.
    pub fn invalidate(&mut self) {
        self.configured = false;
    }

    /// Notes that the surface asked to be reconfigured at the next opportunity.
    pub fn request_reconfigure(&mut self) {
        self.reconfigure_pending = true;
    }

    /// Whether a reconfiguration is owed before the next frame.
    pub fn reconfigure_pending(&self) -> bool {
        self.reconfigure_pending
    }

    /// The formats this surface draws in.
    pub fn formats(&self) -> Formats {
        self.formats
    }

    /// The extent the surface is being presented at.
    ///
    /// An extent that has been asked for and not yet applied is the answer, because that is what
    /// the frame now being built is for: the swap chain is rebuilt before anything is acquired
    /// from it, so no frame ever presents to the extent this stopped reporting.
    pub fn size(&self) -> Size<i32, Device> {
        self.pending
            .unwrap_or_else(|| Size::new(self.config.width as i32, self.config.height as i32))
    }

    /// The surface itself, for acquisition.
    pub fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    /// The view the final copy renders through.
    ///
    /// Where the surface is encoded but can be viewed otherwise, this is the view of its unencoded
    /// twin, and the encode never happens. Everywhere else it is the plain view.
    pub fn present_view(&self, texture: &wgpu::Texture) -> wgpu::TextureView {
        match self.formats.view_format_twin {
            Some(format) => texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                ..Default::default()
            }),
            None => texture.create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{HUMAN_VISIBLE_WAIT, human_visible_wait};

    #[test]
    fn only_human_visible_surface_waits_are_reported_as_stalls() {
        assert!(!human_visible_wait(
            HUMAN_VISIBLE_WAIT - Duration::from_nanos(1)
        ));
        assert!(human_visible_wait(HUMAN_VISIBLE_WAIT));
        assert!(human_visible_wait(
            HUMAN_VISIBLE_WAIT + Duration::from_nanos(1)
        ));
    }
}
