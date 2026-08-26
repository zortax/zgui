//! Copying a colour texture back to the processor, which is how a pixel is asserted on.

use zgui_geom::{Device, Size};

use crate::gpu::device::Gpu;

/// One rectangle of pixels, as the bytes the texture actually holds.
///
/// Nothing is decoded on the way out: a copy out of an encoded texture yields the stored, encoded
/// bytes, so a comparison compares like with like whatever format the texture was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pixels {
    /// Row-major bytes, four per pixel, tightly packed.
    bytes: Vec<u8>,
    /// The extent in pixels.
    size: Size<i32, Device>,
    /// Whether the channels are stored blue first.
    bgra: bool,
}

impl Pixels {
    /// The pixel at `(x, y)` as red, green, blue and alpha, whatever order the texture stores.
    ///
    /// # Panics
    ///
    /// Panics if the coordinates lie outside what was read.
    pub fn rgba(&self, x: i32, y: i32) -> [u8; 4] {
        assert!(
            x >= 0 && y >= 0 && x < self.size.width && y < self.size.height,
            "({x}, {y}) is outside the {:?} that was read",
            self.size
        );
        let offset = ((y * self.size.width + x) * 4) as usize;
        let raw = [
            self.bytes[offset],
            self.bytes[offset + 1],
            self.bytes[offset + 2],
            self.bytes[offset + 3],
        ];
        if self.bgra {
            [raw[2], raw[1], raw[0], raw[3]]
        } else {
            raw
        }
    }

    /// Returns the bytes, row-major and tightly packed, four to a pixel.
    ///
    /// In the order the texture stores them, so [`Pixels::is_bgra`] says which order that is.
    /// [`Pixels::rgba`] is for asserting on one pixel; this is for a caller copying the whole
    /// rectangle somewhere else, where a call per pixel would be millions of calls a frame.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns `true` where the bytes store blue first.
    ///
    /// A caller handing these to something that names its own formats — a scanout, a codec — has
    /// to say which order they are in, and the texture's format is what decided it.
    pub fn is_bgra(&self) -> bool {
        self.bgra
    }

    /// The extent that was read.
    pub fn size(&self) -> Size<i32, Device> {
        self.size
    }

    /// The largest per-channel difference between two readbacks of the same extent.
    ///
    /// # Panics
    ///
    /// Panics if the two are not the same extent, which would otherwise compare a prefix and
    /// report agreement.
    pub fn max_difference(&self, other: &Self) -> u8 {
        assert_eq!(
            self.size, other.size,
            "two readbacks of different extents cannot be compared"
        );
        let mut worst = 0;
        for y in 0..self.size.height {
            for x in 0..self.size.width {
                let (left, right) = (self.rgba(x, y), other.rgba(x, y));
                for channel in 0..4 {
                    worst = worst.max(left[channel].abs_diff(right[channel]));
                }
            }
        }
        worst
    }
}

/// Copies the top-left `size` rectangle of `texture` into memory.
///
/// # Panics
///
/// Panics if the copy cannot be mapped, which on a working device means the queue faulted — there
/// is nothing a caller could do with that but report it.
pub fn read(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
    size: Size<i32, Device>,
) -> Pixels {
    let width = size.width.max(1) as u32;
    let height = size.height.max(1) as u32;
    let padded = padded_bytes_per_row(width);
    let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("zgui.readback"),
        size: u64::from(padded) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zgui.readback"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue().submit([encoder.finish()]);

    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.wait();

    let view = slice.get_mapped_range();
    let mut bytes = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        bytes.extend_from_slice(&view[start..start + (width * 4) as usize]);
    }
    drop(view);
    buffer.unmap();

    Pixels {
        bytes,
        size: Size::new(width as i32, height as i32),
        bgra: matches!(
            format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ),
    }
}

/// A row of `width` pixels, rounded up to the alignment a copy out of a texture requires.
fn padded_bytes_per_row(width: u32) -> u32 {
    let unpadded = width * 4;
    unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

#[cfg(test)]
mod tests {
    use super::{Pixels, padded_bytes_per_row};
    use zgui_geom::Size;

    #[test]
    fn a_row_is_padded_to_the_copy_alignment() {
        assert_eq!(padded_bytes_per_row(64), 256);
        assert_eq!(padded_bytes_per_row(65), 512);
    }

    #[test]
    fn a_blue_first_texture_reads_back_in_red_first_order() {
        let pixels = Pixels {
            bytes: vec![10, 20, 30, 40],
            size: Size::new(1, 1),
            bgra: true,
        };
        assert_eq!(pixels.rgba(0, 0), [30, 20, 10, 40]);
    }
}
