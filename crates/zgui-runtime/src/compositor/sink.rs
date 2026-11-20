//! A texture sink for the atlas zgui still fills.

use zgui_atlas::{SinkError, TextureFormat, TextureId, TextureSink};
use zgui_geom::{Device, Rect, Size};

/// A texture sink that accepts every upload and keeps nothing.
///
/// The compositor rasterises and atlases glyphs itself, so the tiles zgui's own atlas uploads are
/// drawn by nothing.
#[derive(Debug, Default)]
pub(crate) struct Discard;

impl TextureSink for Discard {
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
