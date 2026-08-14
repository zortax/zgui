//! A renderer that accepts a frame and draws nowhere.
//!
//! Everything above it is the real thing — the cascade, the box tree, layout, shaping and the paint
//! stage — because the question these fixtures ask is about dispatch and geometry, and a graphics
//! device would only add a reason for them not to run on a machine that has none.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use zgui::atlas::{SinkError, TextureFormat, TextureId, TextureSink};
use zgui::geom::{Device, Rect, Size};
use zgui::platform::Surface;
use zgui::render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, TextureHandle,
};
use zgui::runtime::AppError;

/// The most vector items any frame drawn through this renderer has carried.
///
/// A path rasteriser is built by the first frame whose display list holds one, and it is then held
/// for the life of the device. So the number worth watching is the high-water mark over a whole
/// fixture rather than the last frame's: an interface that reaches for one only while a panel is
/// opening has still reached for it.
static VECTOR_ITEMS: AtomicUsize = AtomicUsize::new(0);

/// The most vector passes any frame drawn through this renderer has planned.
static VECTOR_PASSES: AtomicUsize = AtomicUsize::new(0);

/// Forgets what earlier frames carried, so one fixture's count is its own.
pub fn forget_vectors() {
    VECTOR_ITEMS.store(0, Ordering::Relaxed);
    VECTOR_PASSES.store(0, Ordering::Relaxed);
}

/// The most vector items and vector passes any frame has carried since [`forget_vectors`].
pub fn vectors_drawn() -> (usize, usize) {
    (
        VECTOR_ITEMS.load(Ordering::Relaxed),
        VECTOR_PASSES.load(Ordering::Relaxed),
    )
}

/// A texture sink that accepts every upload and holds nothing.
struct NullSink;

impl TextureSink for NullSink {
    fn create_texture(
        &mut self,
        _texture: TextureId,
        _size: Size<i32, Device>,
        _format: TextureFormat,
    ) -> Result<(), SinkError> {
        Ok(())
    }

    fn write_texture(
        &mut self,
        _texture: TextureId,
        _bounds: Rect<i32, Device>,
        _format: TextureFormat,
        _bytes: &[u8],
    ) -> Result<(), SinkError> {
        Ok(())
    }

    fn destroy_texture(&mut self, _texture: TextureId) {}
}

/// A renderer that accepts a frame and does nothing with it.
struct NullRenderer {
    /// The surface it was configured for.
    target: Option<RenderTarget>,
    /// Where tiles are uploaded.
    sink: NullSink,
    /// The next handle an external texture is given.
    next: u64,
}

impl Renderer for NullRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
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
        _damage: &zgui::bits::DamageSet,
    ) -> FrameOutcome {
        VECTOR_ITEMS.fetch_max(scene.primitives.vectors.len(), Ordering::Relaxed);
        VECTOR_PASSES.fetch_max(scene.pass_plan().passes.len(), Ordering::Relaxed);
        FrameOutcome::Presented(FrameStats::default())
    }

    fn register_external(&mut self, _texture: ExternalTexture) -> TextureHandle {
        self.next += 1;
        TextureHandle(self.next)
    }

    fn release_external(&mut self, _handle: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        &mut self.sink
    }
}

/// Builds the renderer a window draws through.
pub fn build(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    Ok(Box::new(NullRenderer {
        target: Some(target),
        sink: NullSink,
        next: 0,
    }))
}
