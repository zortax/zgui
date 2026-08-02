//! A renderer that accepts a frame and draws nowhere.
//!
//! It exists so that a budget is the pipeline's own cost: a renderer that recorded the display list
//! would put its own serialisation into every measurement, and a real one would put a graphics
//! driver into it. Everything above it — the cascade, the box tree, layout, shaping, rasterisation
//! and the paint stage — is the real thing, including the atlas uploads, which are performed and
//! discarded rather than skipped.

use std::sync::Arc;

use zgui_geom::{Device, Size};
use zgui_platform::Surface;
use zgui_render::{RenderTarget, Renderer};
use zgui_runtime::AppError;

/// A texture sink that accepts every upload and holds nothing.
struct NullSink;

impl zgui_atlas::TextureSink for NullSink {
    fn create_texture(
        &mut self,
        _texture: zgui_atlas::TextureId,
        _size: Size<i32, Device>,
        _format: zgui_atlas::TextureFormat,
    ) -> Result<(), zgui_atlas::SinkError> {
        Ok(())
    }

    fn write_texture(
        &mut self,
        _texture: zgui_atlas::TextureId,
        _bounds: zgui_geom::Rect<i32, Device>,
        _format: zgui_atlas::TextureFormat,
        _bytes: &[u8],
    ) -> Result<(), zgui_atlas::SinkError> {
        Ok(())
    }

    fn destroy_texture(&mut self, _texture: zgui_atlas::TextureId) {}
}

/// A renderer that accepts a frame and does nothing with it.
///
/// It exists so that a profile is the pipeline's own cost: a renderer that recorded the display
/// list would put its own serialisation into every measurement.
struct NullRenderer {
    /// The surface it was configured for.
    target: Option<RenderTarget>,
    /// Where tiles are uploaded.
    sink: NullSink,
    /// The next handle an external texture is given.
    next: u64,
}

impl Renderer for NullRenderer {
    fn capabilities(&self) -> zgui_render::RenderCapabilities {
        zgui_render::RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(
        &mut self,
        _scene: &zgui_scene::Scene,
        _damage: &zgui_bits::DamageSet,
    ) -> zgui_render::FrameOutcome {
        zgui_render::FrameOutcome::Presented(zgui_render::FrameStats::default())
    }

    fn register_external(
        &mut self,
        _texture: zgui_render::ExternalTexture,
    ) -> zgui_render::TextureHandle {
        self.next += 1;
        zgui_render::TextureHandle(self.next)
    }

    fn release_external(&mut self, _handle: zgui_render::TextureHandle) {}

    fn memory(&self) -> zgui_render::MemoryReport {
        zgui_render::MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
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
