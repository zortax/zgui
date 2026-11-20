//! The destination of the texture uploads zgui's own atlas still queues.

use zgui_atlas::{SinkError, TextureFormat, TextureId, TextureSink};
use zgui_geom::{Device, Rect, Size};

/// A texture sink that accepts every upload and keeps nothing.
///
/// The compositor rasterises and atlases what it draws, so no frame samples these textures.
#[derive(Debug, Default)]
pub(crate) struct Unsent;

impl TextureSink for Unsent {
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
