//! What a window draws through, and what records the display list it drew.

use std::sync::Arc;

use zgui::geom::{Device, Size};
use zgui::platform::Surface;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::AppError;

/// A texture sink that accepts every upload and holds nothing.
struct NullSink;

impl zgui::atlas::TextureSink for NullSink {
    fn create_texture(
        &mut self,
        _texture: zgui::atlas::TextureId,
        _size: Size<i32, Device>,
        _format: zgui::atlas::TextureFormat,
    ) -> Result<(), zgui::atlas::SinkError> {
        Ok(())
    }

    fn write_texture(
        &mut self,
        _texture: zgui::atlas::TextureId,
        _bounds: zgui::geom::Rect<i32, Device>,
        _format: zgui::atlas::TextureFormat,
        _bytes: &[u8],
    ) -> Result<(), zgui::atlas::SinkError> {
        Ok(())
    }

    fn destroy_texture(&mut self, _texture: zgui::atlas::TextureId) {}
}

/// A renderer that accepts a frame and does nothing with it, so the numbers are the CPU's.
struct NullRenderer {
    /// The surface it was configured for.
    target: Option<RenderTarget>,
    /// Where tiles are uploaded.
    sink: NullSink,
    /// The next handle an external texture is given.
    next: u64,
    /// Where a frame drawn against the whole surface leaves its display list.
    full: std::rc::Rc<crate::verify::FullFrame>,
}

impl Renderer for NullRenderer {
    /// Yes, for the same reason every other number this harness takes is a CPU number.
    ///
    /// This renderer holds no pixels, so it cannot really move any. What it can do is let the
    /// *decision* run: a scroll that a real renderer would answer by translating its composed
    /// target narrows the frame's damage, and the emit walk, the replays and the draw-order inserts
    /// that follow from that narrowing are exactly what this harness exists to measure. Answering
    /// false here would measure the frame the framework no longer draws.
    ///
    /// What it does not measure is the copy itself, which is GPU work — as is everything else this
    /// harness leaves out by having no device.
    fn shifts_composed_pixels(&self) -> bool {
        true
    }

    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        zgui::render::RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(
        &mut self,
        scene: &zgui::scene::Scene,
        damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        self.full.observe(scene, damage);
        crate::phase::observe(scene, damage);
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
    }

    fn register_external(
        &mut self,
        _texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        self.next += 1;
        zgui::render::TextureHandle(self.next)
    }

    fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.sink
    }
}

thread_local! {
    /// Where the pixel comparison leaves the readback it asked for.
    pub(crate) static CAPTURE: std::rc::Rc<crate::verify::Capture> = std::rc::Rc::default();

    /// Where each window's display-list recorder is left for its opener to pick up, oldest first.
    ///
    /// A renderer is built inside the window that will draw through it, so this is the only place
    /// the two can be introduced. A slot rather than a list would hold only the window opened
    /// last, and the differential opens two.
    static FULL_FRAMES: std::cell::RefCell<Vec<std::rc::Rc<crate::verify::FullFrame>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The display-list recorder of the window that was opened most recently.
///
/// # Panics
///
/// Panics when no window has been opened since the last call, because a comparison reading a
/// recorder no renderer writes to compares two frames that were never drawn.
pub(crate) fn mounted_recorder() -> std::rc::Rc<crate::verify::FullFrame> {
    FULL_FRAMES.with(|held| {
        held.borrow_mut()
            .pop()
            .expect("a window was opened and built a renderer")
    })
}

/// Builds the renderer a window draws through: nothing, or this machine's device offscreen.
pub(crate) fn renderer(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    // Every window publishes a display-list recorder, whichever renderer it draws through, because
    // the recorder is how a comparison reads what a frame emitted and a phase that opens a window
    // has no way to attach one afterwards. Keeping the list costs nothing until something asks for
    // it: the recorder is off until a phase turns it on.
    let full = std::rc::Rc::new(crate::verify::FullFrame::default());
    FULL_FRAMES.with(|held| held.borrow_mut().push(std::rc::Rc::clone(&full)));
    if std::env::var_os("ZGUI_BENCH_CAPTURE").is_some() {
        let mut gpu = zgui_render_wgpu::Builder::new()
            .offscreen(
                target,
                zgui_render_wgpu::wgpu::TextureFormat::Bgra8Unorm,
                false,
            )
            .map_err(AppError::GpuUnavailable)?;
        zgui_render_vector_vello::attach(&mut gpu, target.size);
        return Ok(Box::new(crate::verify::Listed {
            inner: Box::new(crate::verify::Recorded {
                inner: gpu,
                capture: CAPTURE.with(std::clone::Clone::clone),
            }),
            full,
        }));
    }
    if std::env::var_os("ZGUI_BENCH_GPU").is_none() {
        return Ok(Box::new(NullRenderer {
            target: Some(target),
            sink: NullSink,
            next: 0,
            full,
        }));
    }
    let mut gpu = zgui_render_wgpu::Builder::new()
        .offscreen(
            target,
            zgui_render_wgpu::wgpu::TextureFormat::Bgra8Unorm,
            false,
        )
        .map_err(AppError::GpuUnavailable)?;
    zgui_render_vector_vello::attach(&mut gpu, target.size);
    Ok(Box::new(crate::verify::Listed {
        inner: Box::new(gpu),
        full,
    }))
}
