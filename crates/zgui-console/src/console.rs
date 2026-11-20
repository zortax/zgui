//! What a display is drawn through on a console.
//!
//! A window system presents for a caller; a console does not, so the last step belongs to whatever
//! draws. [`Renderer::draw`] is the frame boundary and the only one: nothing above it is told when
//! a frame begins or ends, and nothing below it is asked to present. [`DrmRenderer`] is that step.
//! It owns a `WgpuRenderer` composing into a texture of its own and the display it draws to,
//! forwards every method, and in [`Renderer::draw`] draws the frame and puts it on the screen.
//!
//! It lives here rather than in the backend because a renderer is built by a [`RendererFactory`],
//! which the runtime owns, and the console backend sits far below the runtime. The backend offers
//! what this needs: the texture format a display can scan out, the map from a surface to the
//! display behind it, and the flip.
//!
//! # What a frame costs
//!
//! One readback of the whole target and one copy of it into a buffer the display scans out of,
//! both on the thread that also reads input. Every machine can run it.
//!
//! # The two methods that are not a plain forward
//!
//! [`Renderer::as_any_mut`] answers with the *inner* renderer, so a backend companion reaches the
//! concrete backend through it.
//!
//! [`Renderer::draw`] is where the presentation happens, and it reports what happened in the
//! contract's own vocabulary. The frame is composed first and offered to the display afterwards,
//! so its damage retires whatever the display answers:
//!
//! - A display that declines the picture because a flip is still on its way answers
//!   [`SkipReason::Declined`], and asking again after one refresh interval is when the buffer is
//!   free.
//! - A readback, a copy or a flip that failed is [`SkipReason::Validation`], the nearest true
//!   statement the contract has: the frame was submitted and something below the renderer refused
//!   to present it.

use std::sync::Arc;

use compositor_api::Renderer;
use compositor_types::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, SkipReason,
    TargetPoolReport, VectorBackend, VectorStatus,
};
use compositor_vello::VelloOptions;
use compositor_wgpu::{Gpu, SharedGraphics, WgpuRenderer};
use tracing::warn;
use zgui_platform::{PlatformError, Surface};
use zgui_platform_drm::{Displays, DrmDisplay};
use zgui_runtime::{AppError, RendererFactory};

/// A renderer that draws a frame and puts it on a display.
///
/// Everything about drawing belongs to the renderer this wraps. What this adds is the last step a
/// console needs and a window system does not.
struct DrmRenderer {
    /// What draws.
    inner: WgpuRenderer,
    /// Where the frame it drew goes.
    display: DrmDisplay,
}

impl DrmRenderer {
    /// Reads the frame `inner` just composed back and copies it into the display's buffer.
    fn present(&self) -> FrameOutcome {
        let Some(pixels) = self.inner.read_presented() else {
            warn!("this renderer kept no presented image, so nothing reaches the display");
            return FrameOutcome::Skipped(SkipReason::Validation);
        };
        let size = pixels.size();
        let pixels = zgui_render_wgpu::Pixels::from_bytes(
            pixels.bytes().to_vec(),
            zgui_geom::Size::new(size.width, size.height),
            pixels.is_bgra(),
        );
        match self.display.present(&pixels) {
            Ok(true) => FrameOutcome::Presented(compositor_types::FrameStats::default()),
            // The buffer this frame would be written into is the one still on the screen. The
            // frame's work is submitted and the target holds it, so the damage retires and another
            // frame is asked for — which the contract paces at one refresh interval, by which time
            // the flip has completed. `Declined` rather than `Timeout`, because this refusal
            // blocked nothing: `Timeout` is what latches a window as starved.
            Ok(false) => FrameOutcome::Skipped(SkipReason::Declined),
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

    fn target_age(&self) -> BufferAge {
        self.inner.target_age()
    }

    /// Draws the frame, reads it back and copies it into the display's buffer.
    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let drawn = self.inner.draw(frame);
        if !matches!(drawn, FrameOutcome::Presented(_)) {
            return drawn;
        }
        match self.present() {
            FrameOutcome::Presented(_) => drawn,
            refused => refused,
        }
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

    /// The renderer underneath, which is the one a backend-specific companion is looking for.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.inner.as_any_mut()
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        self.inner.as_any()
    }
}

/// Makes the renderer factory an application's displays are drawn through on a console.
///
/// One graphics device behind all of them: the factory is called once per display and keeps that
/// device, its pipelines and its compiled-shader cache between calls.
///
/// `displays` is where a display is found: the frame loop writes each one in under the surface it
/// is seen as, and it is the same map [`zgui_platform_drm::run`] was given. Both are one decision,
/// and [`RunDrm::run_drm`](crate::RunDrm::run_drm) takes it.
///
/// # Errors
///
/// The factory returns [`AppError::GpuUnavailable`] when no adapter on this machine could compose a
/// frame, naming every one that was tried, and [`AppError::Platform`] when the surface it is handed
/// belongs to no running frame loop, which is the answer a factory installed on its own gets.
pub(crate) fn factory(graphics: SharedGraphics, displays: Displays) -> RendererFactory {
    Box::new(move |surface, target| renderer(&graphics, &displays, surface, target))
}

/// Opens a renderer for `surface` on the shared graphics.
fn renderer(
    graphics: &SharedGraphics,
    displays: &Displays,
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let id = surface.id();
    let Some(display) = displays.for_surface(id) else {
        return Err(AppError::Platform(PlatformError::Backend(format!(
            "surface {id:?} is not a display any frame loop is driving, so a frame drawn for it \
             would reach no screen"
        ))));
    };
    // The format is the backend's, because the buffers a display scans out of were allocated with
    // the fourcc its channel order picks.
    let mut inner = graphics
        .renderer_offscreen(target.size, zgui_platform_drm::FORMAT)
        .map_err(AppError::GpuUnavailable)?;
    // Without a path rasteriser, paths and glyphs on the path route draw nothing.
    let options = VelloOptions {
        adapter: None,
        cache_directory: zgui_render_wgpu::gpu::pipeline_cache::directory(),
    };
    inner.set_vector_factory(
        Box::new(move |gpu: &Arc<Gpu>, size| {
            compositor_vello::for_device(gpu.adapter(), gpu.device(), gpu.queue(), size, &options)
        }),
        VectorBackend::Compute,
        "vello",
    );
    inner.configure(target);
    Ok(Box::new(DrmRenderer { inner, display }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use zgui_geom::{Device, DevicePx, Size};
    use zgui_platform::SurfaceId;
    use zgui_platform_headless::OffscreenSurface;

    #[test]
    fn the_factory_is_built_without_touching_a_graphics_device() {
        // Making the factory must not enumerate adapters: it is built while the application is
        // still being described, and the device is opened by the first display instead.
        let _factory = factory(SharedGraphics::new(), Displays::new());
    }

    #[test]
    fn a_surface_no_loop_is_driving_is_refused_rather_than_drawn_nowhere() {
        // The map is empty until a loop writes into it, so this is the answer a factory installed
        // without the loop gets. The refusal comes before any adapter is asked for, which is why a
        // machine with no graphics device runs this test.
        let mut factory = factory(SharedGraphics::new(), Displays::new());
        let surface: Arc<dyn Surface> = Arc::new(OffscreenSurface::new(
            SurfaceId::new(1),
            Size::<DevicePx, Device>::new(DevicePx(64.0), DevicePx(64.0)),
        ));
        let target = RenderTarget {
            size: [64, 64],
            scale: 1.0,
            opaque: true,
        };

        match factory(&surface, target) {
            Err(AppError::Platform(PlatformError::Backend(reason))) => assert!(
                reason.contains("would reach no screen"),
                "the refusal has to say where the frame would have gone, and it said: {reason}"
            ),
            Err(other) => panic!("a surface no loop is driving was refused with: {other}"),
            Ok(_) => panic!(
                "a surface no loop is driving has no display to reach, so a renderer for it would \
                 compose frames and put them nowhere"
            ),
        }
    }
}
