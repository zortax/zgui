//! The surface configuration, and the only place `configure` is called.

use std::sync::Arc;

use zgui_geom::{Device, Size};

use crate::gpu::device::Gpu;
use crate::gpu::formats::Formats;
use crate::target::native::{ChainConfig, NativeChain, NativeWindow};
use crate::target::scene_texture::{SHRINK_SLACK, size_class};

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

/// Who waits for the display.
///
/// [`PresentPacing::Display`] lets the graphics API do it, inside the acquisition, on whichever
/// thread asked for the frame. That is right wherever nothing else knows when a frame is due, and
/// it is what makes the display's own cadence the thing frames are paced against.
///
/// [`PresentPacing::Platform`] is for a window integration that paces frames itself. The wait then
/// has to leave the acquisition entirely: on a compositor that throttles the swap chain on its own
/// frame callbacks, a surface it stops drawing receives none, and an acquisition that waits for one
/// blocks the thread that also reads input until the driver's timeout — a freeze rather than a
/// dropped frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PresentPacing {
    /// The graphics API waits for the display.
    #[default]
    Display,
    /// The window integration paces frames, and the acquisition must not block.
    Platform,
}

/// The presentation mode for `pacing`, from what this surface actually offers.
///
/// Waiting is `Fifo`, which every implementation has. Not waiting is `Mailbox` where it exists and
/// `Immediate` where it does not: both hand a texture back without waiting for the display, and
/// `Mailbox` is preferred because it drops the frames it supersedes instead of showing them. A
/// surface offering neither falls back to `Fifo` and the integration's own pacing then sits on top
/// of the driver's, which costs latency but cannot deadlock.
pub fn present_mode(
    pacing: PresentPacing,
    capabilities: &wgpu::SurfaceCapabilities,
) -> wgpu::PresentMode {
    if pacing == PresentPacing::Display {
        return wgpu::PresentMode::Fifo;
    }
    [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate]
        .into_iter()
        .find(|mode| capabilities.present_modes.contains(mode))
        .unwrap_or(wgpu::PresentMode::Fifo)
}

/// What a surface is, as the swap chain has to be configured for it.
///
/// The three travel together because they are decided together, by whoever owns the window, and
/// are re-stated on every candidate adapter a surface is offered to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceSetup {
    /// The extent to configure at, in device pixels.
    pub size: Size<i32, Device>,
    /// Whether the surface is opaque.
    pub opaque: bool,
    /// Who waits for the display.
    pub pacing: PresentPacing,
}

/// A configured surface: the swap chain, the configuration it holds, and whether it is current.
///
/// Every field of the configuration is written here. Leaving one out is a decision by default, and
/// wgpu's defaults for the two that matter — the format and the alpha mode — are "whatever the
/// driver listed first" and "work it out later".
#[derive(Debug)]
pub struct ConfiguredSurface {
    /// The swap chain this crate made for the surface, where it made one.
    ///
    /// Declared before `surface`, so it is dropped first: a swap chain may not outlive the
    /// surface it was made for. While it exists, wgpu's surface is never configured, because a
    /// surface has at most one swap chain.
    native: Option<NativeChain>,
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
    /// The extent frames are drawn at, which is at most the swap chain's.
    used: Size<i32, Device>,
    /// Whether the window shows only the top-left `used` pixels of a larger image.
    ///
    /// The swap chain is then kept at a size class, as the composed target is, and a resize inside
    /// the class rebuilds nothing.
    crop: bool,
    /// The largest extent a texture may have on this device.
    largest: i32,
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
    pub fn new(gpu: &Arc<Gpu>, surface: wgpu::Surface<'static>, setup: SurfaceSetup) -> Self {
        Self::for_window(gpu, surface, setup, NativeWindow::default())
    }

    /// [`ConfiguredSurface::new`], for a surface made for `window`.
    pub(crate) fn for_window(
        gpu: &Arc<Gpu>,
        surface: wgpu::Surface<'static>,
        setup: SurfaceSetup,
        window: NativeWindow,
    ) -> Self {
        let mut configured = Self::unconfigured(gpu, surface, setup, window);
        configured.apply(gpu);
        configured
    }

    /// Picks the format and alpha mode of `surface` for `size`, and keeps the swap chain for the
    /// first [`ConfiguredSurface::apply`].
    ///
    /// A device opener uses the time between the two calls to build the pipelines for
    /// [`ConfiguredSurface::formats`].
    ///
    /// # Panics
    ///
    /// Panics in a debug build for the same reason as [`ConfiguredSurface::new`].
    pub(crate) fn unconfigured(
        gpu: &Arc<Gpu>,
        surface: wgpu::Surface<'static>,
        setup: SurfaceSetup,
        window: NativeWindow,
    ) -> Self {
        let capabilities = surface.get_capabilities(gpu.adapter());
        let mode = present_mode(setup.pacing, &capabilities);
        tracing::info!(
            offered = ?capabilities.present_modes,
            chosen = ?mode,
            pacing = ?setup.pacing,
            frame_latency = FRAME_LATENCY,
            "surface presentation modes"
        );
        let formats = crate::gpu::formats::choose(
            &capabilities.formats,
            &capabilities.alpha_modes,
            setup.opaque,
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
            // `Auto` keeps sRGB, or extended linear sRGB for an fp16 format, which is what the
            // compositing shaders encode for.
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: setup.size.width.max(1) as u32,
            height: setup.size.height.max(1) as u32,
            present_mode: mode,
            desired_maximum_frame_latency: FRAME_LATENCY,
            alpha_mode: formats.alpha_mode,
            view_formats: formats.view_formats(),
        };
        let native = if crate::target::native::wanted() {
            let native = NativeChain::new(gpu, &surface, window);
            if native.is_none() {
                tracing::info!(
                    "this backend or this surface cannot have a swap chain of the renderer's own, \
                     so wgpu's is used"
                );
            }
            native
        } else {
            None
        };
        let mut configured = Self {
            native,
            surface,
            config,
            formats,
            configured: false,
            reconfigure_pending: false,
            pending: None,
            used: Size::new(1, 1),
            crop: false,
            largest: gpu.capabilities().max_texture_size,
        };
        // A swap chain larger than the largest texture fails validation, and the device opener
        // then refuses the adapter. The first frame configures again at the window's real extent.
        let largest = configured.largest;
        configured.resize(Size::new(
            setup.size.width.min(largest),
            setup.size.height.min(largest),
        ));
        configured
    }

    /// Records that the swap chain is wanted at `size`, without rebuilding it yet.
    ///
    /// A resize asks for this at the top of a frame, because everything sized against the surface
    /// — the composed target, the viewport a media query is matched against — is derived from what
    /// the window now is. Rebuilding the swap chain there as well would be an ordering nobody
    /// needs and the most expensive one available: see [`ConfiguredSurface::apply`].
    pub fn resize(&mut self, size: Size<i32, Device>) {
        self.used = Size::new(size.width.max(1), size.height.max(1));
        let extent = self.extent_for(self.used);
        let width = extent.width as u32;
        let height = extent.height as u32;
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
        let began = web_time::Instant::now();
        let chain = self.chain_config();
        match self.native.as_mut().map(|native| native.configure(&chain)) {
            Some(Ok(())) => {}
            Some(Err(reason)) => {
                tracing::warn!(
                    %reason,
                    "the renderer's own swap chain could not be made, so wgpu's is used"
                );
                // Destroyed first: a surface has at most one swap chain.
                self.native = None;
                self.surface.configure(gpu.device(), &self.config);
            }
            None => self.surface.configure(gpu.device(), &self.config),
        }
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

    /// The configuration a native chain is made for, from the one wgpu's would hold.
    fn chain_config(&self) -> ChainConfig {
        ChainConfig {
            size: Size::new(self.config.width as i32, self.config.height as i32),
            format: self.config.format,
            view_formats: self.config.view_formats.clone(),
            present_mode: self.config.present_mode,
            alpha_mode: self.config.alpha_mode,
        }
    }

    /// The swap chain this crate made for the surface, where it made one.
    pub(crate) fn native(&mut self) -> Option<&mut NativeChain> {
        self.native.as_mut()
    }

    /// Whether frames are presented through a swap chain this crate made.
    pub(crate) fn is_native(&self) -> bool {
        self.native.is_some()
    }

    /// The swap chain extent that holds frames of `used` pixels: `used` itself, or with cropping
    /// what [`cropped_extent`] keeps.
    fn extent_for(&self, used: Size<i32, Device>) -> Size<i32, Device> {
        if !self.crop {
            return used;
        }
        let current = Size::new(self.config.width as i32, self.config.height as i32);
        cropped_extent(used, self.configured.then_some(current), self.largest)
    }

    /// Lets the swap chain be larger than the frames drawn into it, for a window that shows only
    /// the frame's extent of each image. Takes effect with the next resize.
    pub fn set_crop(&mut self, crop: bool) {
        self.crop = crop;
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

    /// The presentation mode the swap chain actually holds.
    pub fn present_mode(&self) -> wgpu::PresentMode {
        self.config.present_mode
    }

    /// The extent the surface is being presented at.
    ///
    /// This is the extent of the frames, which a cropping surface shows from a larger swap chain.
    /// It is the extent asked for even before the swap chain is rebuilt, because that is what the
    /// frame now being built is for: the swap chain is rebuilt before anything is acquired from
    /// it, so no frame ever presents to the extent this stopped reporting.
    pub fn size(&self) -> Size<i32, Device> {
        self.used
    }

    /// The extent the swap chain holds, or will hold once it is rebuilt.
    pub fn extent(&self) -> Size<i32, Device> {
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

/// The swap chain extent a cropping surface uses for frames of `used` pixels.
///
/// The `current` extent is kept while `used` fits in it and it holds at most [`SHRINK_SLACK`] times
/// the area of the size class of `used`. Otherwise the answer is that size class, at most `largest`
/// on each axis.
pub(crate) fn cropped_extent(
    used: Size<i32, Device>,
    current: Option<Size<i32, Device>>,
    largest: i32,
) -> Size<i32, Device> {
    let class = Size::new(
        size_class(used.width).min(largest).max(used.width),
        size_class(used.height).min(largest).max(used.height),
    );
    let area = |extent: Size<i32, Device>| extent.width as i64 * extent.height as i64;
    match current {
        Some(current)
            if used.width <= current.width
                && used.height <= current.height
                && area(current) <= SHRINK_SLACK * area(class) =>
        {
            current
        }
        _ => class,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use zgui_geom::Size;

    use super::{HUMAN_VISIBLE_WAIT, cropped_extent, human_visible_wait};

    #[test]
    fn a_drag_inside_a_size_class_keeps_the_swap_chain() {
        let first = cropped_extent(Size::new(640, 480), None, 8192);
        assert_eq!(first, Size::new(768, 512));
        for width in 641..=768 {
            assert_eq!(
                cropped_extent(Size::new(width, 500), Some(first), 8192),
                first,
                "a frame of {width} pixels rebuilt the swap chain"
            );
        }
        assert_eq!(
            cropped_extent(Size::new(769, 500), Some(first), 8192),
            Size::new(1024, 512),
            "a frame that outgrows the class gets the next one"
        );
    }

    #[test]
    fn a_shrink_keeps_the_swap_chain_until_it_wastes_most_of_it() {
        let large = Size::new(1280, 1024);
        assert_eq!(
            cropped_extent(Size::new(1000, 700), Some(large), 8192),
            large
        );
        assert_eq!(
            cropped_extent(Size::new(640, 480), Some(large), 8192),
            Size::new(768, 512)
        );
    }

    #[test]
    fn the_extent_never_passes_the_largest_texture() {
        assert_eq!(
            cropped_extent(Size::new(8000, 100), None, 8192),
            Size::new(8192, 256)
        );
        assert_eq!(
            cropped_extent(Size::new(8100, 100), None, 8100),
            Size::new(8100, 256)
        );
    }

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
