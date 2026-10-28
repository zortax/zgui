//! Copying a colour texture back to the processor, which is how a pixel is asserted on.

use std::ops::Range;

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
    /// Nothing read yet.
    ///
    /// What a caller reading the same surface over and over starts with, so that the bytes are
    /// allocated once and every later frame writes into the ones already there.
    pub fn nothing() -> Self {
        Self {
            bytes: Vec::new(),
            size: Size::new(0, 0),
            bgra: false,
        }
    }

    /// Whether this holds a whole frame of `size` in the given channel order.
    ///
    /// A band-limited readback keeps the rows nothing named, which is only a frame where the rows
    /// it keeps are of the same picture. This is what a caller asks before reading only part of
    /// one; where it answers `false`, the whole frame has to be read.
    pub fn holds(&self, size: Size<i32, Device>, bgra: bool) -> bool {
        self.size == size && self.bgra == bgra && !self.bytes.is_empty()
    }

    /// Makes room for a whole frame of `size`, keeping what is there when the extent is unchanged.
    fn resize(&mut self, size: Size<i32, Device>, bgra: bool) {
        if self.size == size && self.bgra == bgra {
            return;
        }
        self.size = size;
        self.bgra = bgra;
        self.bytes.clear();
        self.bytes
            .resize((size.width.max(0) * size.height.max(0) * 4) as usize, 0);
    }

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
    let mut into = Pixels::nothing();
    let mut staging = Staging::new();
    let whole = crate::frame::damage::every_row(size.height.max(1) as u32);
    read_bands(gpu, texture, format, size, &whole, &mut staging, &mut into);
    into
}

/// The buffer a repeated readback copies through, kept between frames.
///
/// A console reads its whole frame back every time it draws one, and allocating the buffer for it
/// each time costs as much as several milliseconds on a slow device — for memory the last frame
/// had already found.
#[derive(Debug, Default)]
pub struct Staging {
    /// What is held, once something has been read.
    buffer: Option<wgpu::Buffer>,
}

impl Staging {
    /// A staging buffer with nothing allocated.
    pub fn new() -> Self {
        Self::default()
    }

    /// The buffer, grown to `bytes` if what is held is smaller.
    ///
    /// Never shrinks: the caller reads one surface over and over, so the size settles after the
    /// first frame and a resize is what moves it.
    fn of(&mut self, gpu: &Gpu, bytes: u64) -> &wgpu::Buffer {
        let held = self
            .buffer
            .as_ref()
            .is_some_and(|held| held.size() >= bytes);
        if !held {
            self.buffer = Some(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("zgui.readback"),
                size: bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        self.buffer.as_ref().expect("just allocated")
    }
}

/// Copies the rows `bands` names out of `texture` into `into`, through `staging`.
///
/// `into` keeps every row no band names, so a caller reading only what changed still holds a whole
/// frame — which is the point: a band-limited readback is worth having exactly because the rest of
/// the frame is already correct.
///
/// A band reaching past the bottom is cut to it, and one entirely past it is left out. Bands are
/// expected to be disjoint and in order; overlapping ones read the same rows twice and answer the
/// same thing.
///
/// # Panics
///
/// Panics if the copy cannot be mapped, which on a working device means the queue faulted — there
/// is nothing a caller could do with that but report it.
pub fn read_bands(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
    size: Size<i32, Device>,
    bands: &[Range<u32>],
    staging: &mut Staging,
    into: &mut Pixels,
) {
    let width = size.width.max(1) as u32;
    let height = size.height.max(1) as u32;
    let bgra = matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    into.resize(Size::new(width as i32, height as i32), bgra);

    // Cut to the texture and dropped where nothing is left, so that a band naming rows past the
    // bottom is a band that reads what there is rather than a copy the device refuses.
    let bands: Vec<Range<u32>> = bands
        .iter()
        .map(|band| band.start.min(height)..band.end.min(height))
        .filter(|band| band.start < band.end)
        .collect();
    if bands.is_empty() {
        return;
    }

    // One buffer for every band, each starting where the last one ended, so that one submission and
    // one mapping serve all of them. A copy's offset has its own alignment, which is why each band
    // begins on one rather than immediately after its predecessor's last row.
    let padded = padded_bytes_per_row(width);
    let mut offsets = Vec::with_capacity(bands.len());
    let mut used = 0_u64;
    for band in &bands {
        used = used.next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        offsets.push(used);
        used += u64::from(padded) * u64::from(band.end - band.start);
    }
    let buffer = staging.of(gpu, used.max(1));

    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zgui.readback"),
        });
    for (band, offset) in bands.iter().zip(&offsets) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: band.start,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: *offset,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(band.end - band.start),
                },
            },
            wgpu::Extent3d {
                width,
                height: band.end - band.start,
                depth_or_array_layers: 1,
            },
        );
    }
    zgui_profile::latency::mark("rb.encoded");
    gpu.queue().submit([encoder.finish()]);

    let slice = buffer.slice(..used);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.wait();
    zgui_profile::latency::mark("rb.mapped");

    let view = slice.get_mapped_range();
    let row = (width * 4) as usize;
    for (band, offset) in bands.iter().zip(&offsets) {
        for (index, line) in (band.start..band.end).enumerate() {
            let from = *offset as usize + index * padded as usize;
            let to = line as usize * row;
            into.bytes[to..to + row].copy_from_slice(&view[from..from + row]);
        }
    }
    drop(view);
    buffer.unmap();
    zgui_profile::latency::mark("rb.copied");
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
