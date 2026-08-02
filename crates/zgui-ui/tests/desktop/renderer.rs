//! A renderer that accepts a frame and draws nowhere.
//!
//! Everything above it is the real thing — the cascade, the box tree, layout, shaping and the paint
//! stage — because the question these fixtures ask is about dispatch and geometry, and a graphics
//! device would only add a reason for them not to run on a machine that has none.

use std::sync::Arc;

use zgui::atlas::{SinkError, TextureFormat, TextureId, TextureSink};
use zgui::geom::{Device, Rect, Size};
use zgui::platform::Surface;
use zgui::render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, TextureHandle,
};
use zgui::runtime::AppError;

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
        _scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> FrameOutcome {
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
