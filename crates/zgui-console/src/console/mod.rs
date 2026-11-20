//! What a display is drawn through on a console.
//!
//! A window system presents for a caller; a console does not, so the last step belongs to whatever
//! draws. [`Renderer::draw`] is the frame boundary and the only one: nothing above it is told when
//! a frame begins or ends, and nothing below it is asked to present. [`DrmRenderer`] is that step.
//! It owns the compositor's `WgpuRenderer` and the display it draws to, forwards every method, and
//! in [`Renderer::draw`] draws the frame and puts it on the screen.
//!
//! It lives here rather than in the backend because a renderer is built by a [`RendererFactory`],
//! which the runtime owns, and the console backend sits far below the runtime. The backend offers
//! the four things this needs: the texture format a display can scan out, the map from a surface to
//! the display behind it, the buffers a display hands out, and the flip.
//!
//! # What a frame costs
//!
//! [`Delivery`] is the decision, taken once when the renderer is built and never per frame.
//!
//! **Drawn.** No readback. The display hands out the buffers it scans out of, the renderer composes
//! into its own target and copies what changed into whichever buffer is free, and the frame
//! reaches the screen where it already lies. [`bracket`] holds the order of that frame.
//!
//! **Copied.** One readback of the whole target and one copy of it into a buffer the display scans
//! out of, both on the thread that also reads input. This is the path for a display that cannot
//! hand out its own buffers, and every machine can run it.
//!
//! # A frame that changed nothing
//!
//! The compositor builds no frame when nothing changed, and the runtime then asks
//! [`Renderer::present_held`]. On the drawn path that gives over a frame that waited for a free
//! buffer. On the copied path it puts the last picture on the screen again where the display's own
//! frames carry the pointer, because there the picture is what moves the pointer.
//!
//! # The methods that are not a plain forward
//!
//! [`Renderer::as_any_mut`] answers with the *inner* renderer, so `zgui-wgpu` reaches the
//! compositor's wgpu renderer through it to fill `surface` elements.
//!
//! A readback, a handover or a flip that failed after the frame was composed is
//! [`SkipReason::Validation`], the nearest true statement the contract has: the frame was submitted
//! and something below the renderer refused to present it.

mod bracket;
mod device;

pub(crate) use self::device::{graphics, scanout_device};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use compositor_api::Renderer;
use compositor_types::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, SkipReason,
    TargetPoolReport, VectorBackend, VectorStatus,
};
use compositor_vello::VelloOptions;
use compositor_wgpu::{Gpu, SharedGraphics, Supplied, WgpuRenderer};
use tracing::warn;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_platform::{PlatformError, Surface};
use zgui_platform_drm::{Displays, DrmDisplay};
use zgui_runtime::{AppError, RendererFactory};

use self::bracket::drawn_into_scanout;

/// How a composed frame reaches the display.
///
/// Settled when the renderer is built, from what the display hands out, and read once per frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Delivery {
    /// The frame is copied into the display's own buffer, and the buffer is given over.
    Drawn,
    /// The frame is read back out of the renderer and copied into the display's buffer.
    Copied,
}

/// A renderer that draws a frame and puts it on a display.
///
/// Everything about drawing belongs to the renderer this wraps. What this adds is the last step a
/// console needs and a window system does not.
struct DrmRenderer {
    /// What draws.
    inner: WgpuRenderer,
    /// Where the frame it drew goes.
    display: DrmDisplay,
    /// How it gets there.
    delivery: Delivery,
    /// Whether `inner` may copy a composed frame into the buffer selected for it, which its
    /// present gate reads.
    free: Arc<AtomicBool>,
    /// Whether a composed frame waits for a free buffer.
    held: bool,
}

impl DrmRenderer {
    /// Draws the frame into the renderer's target and copies it into a free buffer of the display.
    fn drawn(&mut self, frame: &Frame) -> FrameOutcome {
        let Self {
            inner,
            display,
            free,
            ..
        } = self;
        let outcome = drawn_into_scanout(display, |slot| {
            point_at(inner, free, slot);
            let drawn = inner.draw(frame);
            (drawn, damage_of(inner))
        });
        self.held = outcome == FrameOutcome::Skipped(SkipReason::Declined);
        outcome
    }

    /// Gives over the frame that waited for a free buffer, if one waits.
    fn drawn_held(&mut self) -> FrameOutcome {
        if !self.held {
            return FrameOutcome::Skipped(SkipReason::Undamaged);
        }
        let Self {
            inner,
            display,
            free,
            ..
        } = self;
        let outcome = drawn_into_scanout(display, |slot| {
            point_at(inner, free, slot);
            let presented = inner.present_held();
            (presented, damage_of(inner))
        });
        self.held = outcome == FrameOutcome::Skipped(SkipReason::Declined);
        outcome
    }

    /// Draws the frame, reads it back and copies it into the display's buffer.
    fn copied(&mut self, frame: &Frame) -> FrameOutcome {
        let drawn = self.inner.draw(frame);
        if !matches!(drawn, FrameOutcome::Presented(_)) {
            return drawn;
        }
        match self.present() {
            FrameOutcome::Presented(_) => drawn,
            refused => refused,
        }
    }

    /// Reads the frame `inner` presented last back and copies it into the display's buffer.
    fn present(&self) -> FrameOutcome {
        let Some(pixels) = self.inner.read_presented() else {
            warn!("this renderer kept no presented image, so nothing reaches the display");
            return FrameOutcome::Skipped(SkipReason::Validation);
        };
        match self.display.present(&pixels) {
            Ok(true) => FrameOutcome::Presented(compositor_types::FrameStats::default()),
            // The buffer this frame would be written into is the one still on the screen. The
            // frame's work is submitted and the target holds it, so the damage retires and another
            // frame is asked for. `Declined` rather than `Timeout`, because this refusal blocked
            // nothing: `Timeout` is what latches a window as starved.
            Ok(false) => FrameOutcome::Skipped(SkipReason::Declined),
            Err(error) => {
                warn!("a frame could not be put on its display: {error}");
                FrameOutcome::Skipped(SkipReason::Validation)
            }
        }
    }
}

/// Points the next copy `inner` makes at the buffer in `slot`, and opens its present gate `free`,
/// or holds the frame back when there is no buffer.
fn point_at(inner: &mut WgpuRenderer, free: &AtomicBool, slot: Option<usize>) {
    let selected = slot.is_some_and(|slot| {
        let supplied = inner
            .wgpu_presenter()
            .as_any_mut()
            .and_then(|presenter| presenter.downcast_mut::<Supplied>());
        supplied.is_some_and(|supplied| supplied.select(slot as u32))
    });
    if slot.is_some() && !selected {
        warn!(
            "this renderer was not built over the buffers this display hands out, so the frame \
             is held rather than copied wherever the last one went"
        );
    }
    free.store(selected, Ordering::Relaxed);
}

/// Returns what the frame `inner` presented last changed, as the rectangles a display is told.
///
/// The whole frame is an empty list.
fn damage_of(inner: &WgpuRenderer) -> Vec<Rect<i32, Device>> {
    inner
        .presented_damage()
        .rects()
        .iter()
        .map(|rect| {
            Rect::new(
                Point::new(rect.origin.x, rect.origin.y),
                Size::new(rect.size.width, rect.size.height),
            )
        })
        .collect()
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

    /// Draws the frame and puts it on the display, the way this renderer's [`Delivery`] says.
    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        match self.delivery {
            Delivery::Drawn => self.drawn(frame),
            Delivery::Copied => self.copied(frame),
        }
    }

    /// Gives over a frame that waited for a free buffer, or puts the last picture up again where
    /// the display's own frames carry the pointer.
    fn present_held(&mut self) -> FrameOutcome {
        match self.delivery {
            Delivery::Drawn => self.drawn_held(),
            Delivery::Copied if self.display.carries_the_pointer() => self.present(),
            Delivery::Copied => FrameOutcome::Skipped(SkipReason::Undamaged),
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
/// One graphics device behind all of them: `graphics` is opened before the first display exists,
/// because the images a display scans out of are created on it. The factory is called once per
/// display and keeps that device, its pipelines and its compiled-shader cache between calls.
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

    // The one question that decides the path, asked once. A display that hands out buffers is one
    // whose frames are copied into them; an empty answer is a display that reads its frames back.
    let textures = display.textures();
    let free = Arc::new(AtomicBool::new(false));
    let (mut inner, delivery) = if textures.is_empty() {
        // The format is the backend's, because the buffers a display scans out of were allocated
        // with the fourcc its channel order picks.
        let inner = graphics
            .renderer_offscreen(target.size, zgui_platform_drm::FORMAT)
            .map_err(AppError::GpuUnavailable)?;
        (inner, Delivery::Copied)
    } else {
        let mut inner = Supplied::renderer(graphics, textures).map_err(AppError::GpuUnavailable)?;
        // A frame is copied into a buffer only once the display named a free one.
        let gate = Arc::clone(&free);
        inner.set_present_gate(Box::new(move || gate.load(Ordering::Relaxed)));
        (inner, Delivery::Drawn)
    };
    // Without a path rasteriser, paths and glyphs on the path route draw nothing.
    let options = VelloOptions {
        adapter: None,
        cache_directory: zgui::app::pipeline_directory(),
    };
    inner.set_vector_factory(
        Box::new(move |gpu: &Arc<Gpu>, size| {
            compositor_vello::for_device(gpu.adapter(), gpu.device(), gpu.queue(), size, &options)
        }),
        VectorBackend::Compute,
        "vello",
    );
    inner.configure(target);
    Ok(Box::new(DrmRenderer {
        inner,
        display,
        delivery,
        free,
        held: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use zgui_geom::DevicePx;
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
