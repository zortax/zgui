//! A sink made of byte vectors, so the whole policy runs without a device.

use rustc_hash::FxHashMap;
use zgui_geom::{Device, Rect, Size};

use crate::sink::TextureSink;
use crate::sink::error::SinkError;
use crate::texture::{TextureFormat, TextureId};

/// One texture's worth of bytes.
#[derive(Clone, Debug)]
struct Stored {
    /// The texture's extent, in texels.
    size: Size<i32, Device>,
    /// The format its texels are in.
    format: TextureFormat,
    /// Row-major texels, `size.height` rows of `size.width * format.bytes_per_texel()` bytes.
    texels: Vec<u8>,
}

/// A [`TextureSink`] that keeps its textures in memory.
///
/// It is the reference implementation of the trait and the one every atlas test runs against: the
/// allocation, reference-counting, eviction and upload behaviour under test is the same code a GPU
/// sink would drive, so a bug in that code fails here in milliseconds instead of in a frame
/// capture.
///
/// It also reads back, which a GPU sink does not do cheaply, so a test can assert that the right
/// bytes landed in the right rectangle.
///
/// ```
/// use zgui_atlas::{MemorySink, TextureFormat, TextureId, TextureKind, TextureSink};
/// use zgui_geom::{Point, Rect, Size};
///
/// let mut sink = MemorySink::new();
/// let texture = TextureId::new(TextureKind::Mono, 0);
/// sink.create_texture(texture, Size::new(4, 4), TextureFormat::R8Unorm).unwrap();
/// sink.write_texture(
///     texture,
///     Rect::new(Point::new(1, 1), Size::new(2, 2)),
///     TextureFormat::R8Unorm,
///     &[7, 7, 7, 7],
/// )
/// .unwrap();
///
/// assert_eq!(sink.texel(texture, 1, 1), Some(&[7][..]));
/// assert_eq!(sink.texel(texture, 0, 0), Some(&[0][..]));
/// ```
#[derive(Clone, Debug, Default)]
pub struct MemorySink {
    /// The live textures.
    textures: FxHashMap<TextureId, Stored>,
    /// How many bytes have been written since construction, across every texture.
    bytes_written: u64,
    /// How many write calls have been made since construction.
    writes: u64,
    /// How many textures have ever been created.
    textures_created: u64,
}

impl MemorySink {
    /// A sink holding no textures.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many textures currently exist.
    pub fn live_textures(&self) -> usize {
        self.textures.len()
    }

    /// How many bytes of texel data have been written since construction.
    ///
    /// This does not go down when a texture is destroyed: it is a measure of upload traffic, which
    /// is what an atlas policy is trying to keep small.
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// How many separate writes have been made since construction.
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// How many textures have ever been created, including ones since destroyed.
    ///
    /// This is the number that grows when tile space is not reclaimed: an atlas that never returns
    /// a rectangle keeps needing fresh textures for content that would have fitted in the ones it
    /// already has.
    pub fn textures_created(&self) -> u64 {
        self.textures_created
    }

    /// The extent `texture` was created with, if it still exists.
    pub fn size_of(&self, texture: TextureId) -> Option<Size<i32, Device>> {
        self.textures.get(&texture).map(|stored| stored.size)
    }

    /// The bytes of one texel, or `None` when the texture does not exist or the coordinates lie
    /// outside it.
    pub fn texel(&self, texture: TextureId, x: i32, y: i32) -> Option<&[u8]> {
        let stored = self.textures.get(&texture)?;
        if x < 0 || y < 0 || x >= stored.size.width || y >= stored.size.height {
            return None;
        }
        let stride = stored.format.bytes_per_texel() as usize;
        let offset = (y as usize * stored.size.width as usize + x as usize) * stride;
        Some(&stored.texels[offset..offset + stride])
    }
}

impl TextureSink for MemorySink {
    fn create_texture(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
    ) -> Result<(), SinkError> {
        if size.width <= 0 || size.height <= 0 {
            return Err(SinkError::new(format!(
                "a texture must have a positive extent, not {}x{}",
                size.width, size.height
            )));
        }
        let bytes = format.bytes_for(size.width as u32, size.height as u32) as usize;
        self.textures_created += 1;
        self.textures.insert(
            texture,
            Stored {
                size,
                format,
                texels: vec![0; bytes],
            },
        );
        Ok(())
    }

    fn write_texture(
        &mut self,
        texture: TextureId,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError> {
        let stored = self
            .textures
            .get_mut(&texture)
            .ok_or_else(|| SinkError::new(format!("texture {texture:?} does not exist")))?;
        let stride = format.bytes_per_texel() as usize;
        let row = bounds.size.width as usize * stride;
        if bytes.len() != row * bounds.size.height as usize {
            return Err(SinkError::new(format!(
                "a {}x{} write needs {} bytes, not {}",
                bounds.size.width,
                bounds.size.height,
                row * bounds.size.height as usize,
                bytes.len()
            )));
        }
        for line in 0..bounds.size.height {
            let source = line as usize * row;
            let target = ((bounds.origin.y + line) as usize * stored.size.width as usize
                + bounds.origin.x as usize)
                * stride;
            stored.texels[target..target + row].copy_from_slice(&bytes[source..source + row]);
        }
        self.bytes_written += bytes.len() as u64;
        self.writes += 1;
        Ok(())
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        self.textures.remove(&texture);
    }
}
