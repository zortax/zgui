//! The one seam where atlas policy meets a device.

pub mod error;
pub mod memory;

pub(crate) mod queue;

use zgui_geom::{Device, Rect, Size};

use crate::texture::{TextureFormat, TextureId};

pub use crate::sink::error::SinkError;
pub use crate::sink::memory::MemorySink;

/// Somewhere atlas textures can be created, written to and destroyed.
///
/// Everything about *which* tile goes *where* is decided before this trait is reached, so an
/// implementation is a thin adapter over whatever holds texels — a GPU device, or the plain byte
/// vectors of [`MemorySink`]. That split is what lets allocation, reference counting and eviction
/// be tested exhaustively without a device.
///
/// Calls arrive in a fixed order: a texture is created before anything is written to it, every
/// write lies inside the size it was created with, and nothing is written to a destroyed texture.
/// An implementation may rely on that rather than re-checking it.
pub trait TextureSink {
    /// Creates the texture `texture` with room for `size` texels of `format`.
    ///
    /// Called at most once per [`TextureId`], and always before any write to it.
    fn create_texture(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
    ) -> Result<(), SinkError>;

    /// Creates `texture` with `mip_levels` levels of detail, level zero sized `size`.
    ///
    /// The default ignores the levels and creates the texture plain, which is correct for a sink
    /// that only ever samples level zero — every level's write arrives through
    /// [`TextureSink::write_texture_mip`], and the default there drops the ones such a sink has
    /// nowhere to put.
    fn create_texture_with_mips(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
        mip_levels: u32,
    ) -> Result<(), SinkError> {
        let _ = mip_levels;
        self.create_texture(texture, size, format)
    }

    /// Starts a group of texture writes.
    ///
    /// The default is deliberately empty. Device sinks may use the boundary to combine many
    /// writes into reusable staging storage and one submission.
    fn begin_uploads(&mut self) -> Result<(), SinkError> {
        Ok(())
    }

    /// Writes `bytes` into `bounds` of `texture`.
    ///
    /// The bytes are tightly packed rows of `format` texels, top row first, with no padding
    /// between rows: exactly `format.bytes_for(width, height)` of them.
    fn write_texture(
        &mut self,
        texture: TextureId,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError>;

    /// Writes `bytes` into `bounds` of `texture`'s level `mip`.
    ///
    /// `bounds` is in the coordinate space of that level. The default forwards level zero to
    /// [`TextureSink::write_texture`] and accepts the rest without storing them, which is the
    /// honest behaviour of a sink whose textures have one level.
    fn write_texture_mip(
        &mut self,
        texture: TextureId,
        mip: u32,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError> {
        if mip == 0 {
            return self.write_texture(texture, bounds, format, bytes);
        }
        Ok(())
    }

    /// Finishes the group begun by [`TextureSink::begin_uploads`].
    fn finish_uploads(&mut self) {}

    /// Releases `texture` and everything in it.
    ///
    /// It cannot fail: destruction happens while an atlas is shrinking or being torn down, and
    /// there is nothing useful a caller could do with an error there.
    fn destroy_texture(&mut self, texture: TextureId);
}

/// A borrowed sink is a sink.
///
/// [`Atlas`](crate::Atlas) takes its sink by generic reference so that a caller holding a concrete
/// one pays no indirection. This is what lets a caller holding `&mut dyn TextureSink` — anything
/// that reached the sink through a renderer, whose device type is not statically known — use the
/// same methods.
impl<S: TextureSink + ?Sized> TextureSink for &mut S {
    fn create_texture(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
    ) -> Result<(), SinkError> {
        (**self).create_texture(texture, size, format)
    }

    fn create_texture_with_mips(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
        mip_levels: u32,
    ) -> Result<(), SinkError> {
        (**self).create_texture_with_mips(texture, size, format, mip_levels)
    }

    fn write_texture(
        &mut self,
        texture: TextureId,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError> {
        (**self).write_texture(texture, bounds, format, bytes)
    }

    fn write_texture_mip(
        &mut self,
        texture: TextureId,
        mip: u32,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError> {
        (**self).write_texture_mip(texture, mip, bounds, format, bytes)
    }

    fn begin_uploads(&mut self) -> Result<(), SinkError> {
        (**self).begin_uploads()
    }

    fn finish_uploads(&mut self) {
        (**self).finish_uploads();
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        (**self).destroy_texture(texture);
    }
}
