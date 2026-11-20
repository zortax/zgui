//! Atlas uploads held until there is a device to send them to.

use zgui_atlas::{SinkError, TextureFormat, TextureId, TextureSink};
use zgui_geom::{Device, Rect, Size};

/// A texture sink that keeps every call, in order, until [`Uploads::replay`] sends them on.
#[derive(Debug, Default)]
pub struct Uploads {
    /// The calls not sent on yet, oldest first.
    held: Vec<Upload>,
}

/// One call to a texture sink.
#[derive(Debug)]
enum Upload {
    Create {
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
        mip_levels: u32,
    },
    Write {
        texture: TextureId,
        mip: u32,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: Vec<u8>,
    },
    Destroy(TextureId),
}

impl Uploads {
    /// Sends every held call on to `sink`, oldest first, and forgets them.
    ///
    /// # Errors
    ///
    /// Returns the first error `sink` reports. The calls after it are dropped.
    pub fn replay(&mut self, sink: &mut dyn TextureSink) -> Result<(), SinkError> {
        for upload in self.held.drain(..) {
            match upload {
                Upload::Create {
                    texture,
                    size,
                    format,
                    mip_levels,
                } => sink.create_texture_with_mips(texture, size, format, mip_levels)?,
                Upload::Write {
                    texture,
                    mip,
                    bounds,
                    format,
                    bytes,
                } => sink.write_texture_mip(texture, mip, bounds, format, &bytes)?,
                Upload::Destroy(texture) => sink.destroy_texture(texture),
            }
        }
        Ok(())
    }
}

impl TextureSink for Uploads {
    fn create_texture(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
    ) -> Result<(), SinkError> {
        self.create_texture_with_mips(texture, size, format, 1)
    }

    fn create_texture_with_mips(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
        mip_levels: u32,
    ) -> Result<(), SinkError> {
        self.held.push(Upload::Create {
            texture,
            size,
            format,
            mip_levels,
        });
        Ok(())
    }

    fn write_texture(
        &mut self,
        texture: TextureId,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError> {
        self.write_texture_mip(texture, 0, bounds, format, bytes)
    }

    fn write_texture_mip(
        &mut self,
        texture: TextureId,
        mip: u32,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError> {
        self.held.push(Upload::Write {
            texture,
            mip,
            bounds,
            format,
            bytes: bytes.to_vec(),
        });
        Ok(())
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        self.held.push(Upload::Destroy(texture));
    }
}
