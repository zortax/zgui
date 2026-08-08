//! What a display is drawn through.
//!
//! [`Renderer::draw`] is the frame boundary and the only one: nothing above it is told when a frame
//! begins or ends, and nothing below it is asked to present. So putting a frame on a display is a
//! decorator over the renderer that drew it — [`DrmRenderer`] owns a `WgpuRenderer` and the display
//! it draws to, forwards every method, and in [`Renderer::draw`] draws, reads the target back, and
//! flips to it.
//!
//! # What a frame costs here
//!
//! One readback of the whole target and one copy of it into a buffer the display scans out of. The
//! copy is what a console with no graphics-aware display protocol costs: the kernel scans out of a
//! dumb buffer, and a texture on the graphics device is not one. Nothing between the two is
//! avoidable until this backend imports the renderer's own memory as a framebuffer.
//!
//! # The two methods that are not a plain forward
//!
//! [`Renderer::as_any_mut`] answers with the *inner* renderer. It is the door `zgui-wgpu` reaches
//! the concrete backend through to fill `surface` elements, and a decorator answering with itself
//! would leave every one of those elements empty while nothing reported a fault.
//!
//! [`Renderer::draw`] is where the presentation happens, and it reports what happened in the
//! contract's own vocabulary. A frame the display declined because a flip is still on its way is
//! [`SkipReason::Timeout`]: the work was submitted, the target holds it, and asking again after one
//! refresh interval is exactly when the buffer is free. A readback or a flip that *failed* is
//! [`SkipReason::Validation`], which is the nearest true statement the contract has — the frame was
//! submitted and something below the renderer refused to present it. No variant names a kernel
//! that refused a page flip.

use std::sync::Arc;

use tracing::warn;
use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::{PlatformError, Surface};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    ScrollShift, SkipReason, TargetPoolReport, TextureHandle, VectorStatus,
};
use zgui_render_wgpu::{SharedGraphics, WgpuRenderer, wgpu};
use zgui_runtime::{AppError, RendererFactory};
use zgui_scene::Scene;

use crate::display::{DrmDisplay, driven};

/// The texture a frame is composed into.
///
/// Eight bits a channel and blue first. Unsigned normalised rather than sRGB, because a scanout
/// hands the bytes to the display as they are and a second encoding would lighten every frame.
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// Whether [`FORMAT`] stores its channels blue first.
///
/// This is what a scanout picks its fourcc from, once, when the mode is set. It is derived from the
/// format rather than stated a second time: a frame read back in the other order reaches the screen
/// with its red and blue exchanged, and nothing at all reports it.
pub(crate) const BGRA: bool = matches!(
    FORMAT,
    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
);

// Checked where the decision is made rather than in a test, because it is the sort of mistake that
// compiles: a `FORMAT` changed to a red-first texture leaves every frame on the screen with its red
// and blue exchanged, and nothing at all reports it.
const _: () = assert!(
    BGRA,
    "a scanout copies a frame rather than swizzling it, so the texture has to be blue first"
);

/// A renderer that draws into a texture and puts what it drew on a display.
///
/// Everything about drawing belongs to the renderer this wraps. What this adds is the last step a
/// console needs and a window system does not: the composed frame is read back and copied into the
/// buffer the display is about to scan out of.
pub struct DrmRenderer {
    /// What draws.
    inner: WgpuRenderer,
    /// Where the frame it drew goes.
    display: DrmDisplay,
}

impl DrmRenderer {
    /// A renderer that draws with `inner` and presents to `display`.
    ///
    /// `inner` must compose into a texture rather than into a window surface: what is presented
    /// here is what [`WgpuRenderer::read_presented`] answers, and a window's surface answers
    /// nothing. [`factory`] is what pairs the two correctly.
    pub fn new(inner: WgpuRenderer, display: DrmDisplay) -> Self {
        Self { inner, display }
    }

    /// Reads the frame back and puts it on the display, reporting what happened.
    fn present(&self, drawn: FrameOutcome) -> FrameOutcome {
        let Some(pixels) = self.inner.read_presented() else {
            warn!(
                "this renderer composes into a window surface rather than into a texture, so \
                 there is nothing to read back and nothing reaches the display"
            );
            return FrameOutcome::Skipped(SkipReason::Validation);
        };
        match self.display.present(&pixels) {
            Ok(true) => drawn,
            // The buffer this frame would be written into is the one still on the screen. The
            // frame's work is submitted and the target holds it, so the damage retires and another
            // frame is asked for — which the contract paces at one refresh interval, by which time
            // the flip has completed.
            Ok(false) => FrameOutcome::Skipped(SkipReason::Timeout),
            Err(error) => {
                warn!("a frame could not be put on its display: {error}");
                FrameOutcome::Skipped(SkipReason::Validation)
            }
        }
    }
}

impl Renderer for DrmRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    /// Draws the frame, then puts it on the display.
    ///
    /// Only a frame that reached the texture is presented. Every other outcome either drew nothing
    /// — an unconfigured surface, a device that could not be rebuilt — or drew nothing new, and
    /// putting the texture on the display again would spend a flip showing the picture that is
    /// already there.
    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let drawn = self.inner.draw(scene, damage);
        if matches!(drawn, FrameOutcome::Presented(_)) {
            self.present(drawn)
        } else {
            drawn
        }
    }

    fn shifts_composed_pixels(&self) -> bool {
        self.inner.shifts_composed_pixels()
    }

    fn shift_composed(&mut self, shift: ScrollShift) {
        self.inner.shift_composed(shift);
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn vector_status(&self) -> VectorStatus {
        self.inner.vector_status()
    }

    fn target_pool(&self) -> TargetPoolReport {
        self.inner.target_pool()
    }

    fn release_cached_targets(&mut self) -> u64 {
        self.inner.release_cached_targets()
    }

    fn release_idle_resources(&mut self) -> u64 {
        self.inner.release_idle_resources()
    }

    fn acquire_block(&self) -> core::time::Duration {
        self.inner.acquire_block()
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        self.inner.texture_sink()
    }

    /// The renderer underneath, which is the one a backend-specific companion is looking for.
    ///
    /// Answering with this decorator instead would compile, pass every test that does not draw a
    /// `surface` element, and leave every one that does as empty space.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.inner.as_any_mut()
    }
}

/// The renderer factory an application's displays are drawn through on a console.
///
/// One graphics device behind all of them: the factory is called once per display and keeps the
/// device, its pipelines and its compiled-shader cache between calls. The device itself is opened
/// by the first display, because which adapter is usable is only known against something to draw
/// into.
///
/// Every renderer composes into a texture rather than into a window surface. wgpu answers a DRM
/// window handle with "not a Vulkan-compatible handle", which is a true report of where the gap is,
/// so the frame goes to the display through a readback and a copy instead.
///
/// This and `zgui_platform_drm::run` are one decision: the loop is what says which display a
/// surface is, and it says so only while it runs. `App::run_drm` installs both.
///
/// # Errors
///
/// The factory returns [`AppError::GpuUnavailable`] when no adapter on this machine could compose a
/// frame, naming every one that was tried, and [`AppError::Platform`] when the surface it is handed
/// belongs to no running frame loop — which is what installing this factory on its own looks like.
pub fn factory() -> RendererFactory {
    let graphics = SharedGraphics::new();
    Box::new(move |surface, target| renderer(&graphics, surface, target))
}

/// Opens a renderer for `surface` on the shared graphics.
fn renderer(
    graphics: &SharedGraphics,
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let id = surface.id();
    let Some(display) = driven(id) else {
        return Err(AppError::Platform(PlatformError::Backend(format!(
            "surface {id:?} is not a display any frame loop on this thread is driving, so a \
             frame drawn for it would reach no screen"
        ))));
    };

    // Mutable texture formats are not asked for: nothing here views the target under a second
    // format, and asking would cost the sRGB fast path on adapters that offer one.
    let mut inner = graphics.renderer_offscreen(target, FORMAT, false)?;
    // Without this a display list's vector passes are planned, counted and then drawn from nothing,
    // so every drawing on the display — every icon — is empty space. Which rasteriser this is comes
    // from what the device turned out to be able to run, and the fallback needs no compute shaders.
    zgui_render_vector_vello::attach(&mut inner, target.size);
    Ok(Box::new(DrmRenderer::new(inner, display)))
}
