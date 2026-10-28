//! The textures a frame's side tables are read out of.
//!
//! A side table is read at an index the instance carries — a clip, a paint, a ramp's stops, a
//! transform — so what it needs is random access from both shader stages. A storage buffer is the
//! obvious way to get that and it is not the portable one: storage buffers arrive in OpenGL 4.3 and
//! in OpenGL ES 3.1, so a GL 3.3 context has none, and neither does WebGL 2. On such a device
//! `create_bind_group_layout` refuses the layout outright and the renderer opens nothing at all.
//!
//! A texture read with `textureLoad` gives the same random access everywhere. The alternative was a
//! uniform block, which is faster still on some hardware and carries 64 KB at most — about a
//! thousand transforms, or seven hundred clips, and then a cliff in the middle of a frame. A table
//! this wide reaches 8192 rows before any device's limit, which is thirty-two megabytes a table and
//! past anything a document can produce.
//!
//! # What it costs
//!
//! Measured against a storage buffer over twenty thousand instances, with the per-fragment reads a
//! real frame makes: no difference on a discrete GPU, and about a fifth slower on a software
//! rasteriser. What pays for it is the draw order moving to a vertex attribute, which takes an
//! indirection out of the vertex stage — the fetch a tiled mobile GPU charges twice.

use bytemuck::Pod;

use crate::gpu::device::Gpu;

/// How many texels wide every table is.
///
/// A power of two, so the index a shader holds splits into a column and a row with a mask and a
/// shift rather than a division. It is also what `common.wgsl` divides by, and the two have to
/// agree — `TEXELS_WIDE` is written into the shader preamble rather than typed there twice.
pub const TEXELS_WIDE: u32 = 256;

/// How many bytes one texel holds.
///
/// `rgba32uint`: four thirty-two-bit channels. Every structure a table holds is a multiple of four
/// bytes, so a texel is a whole number of fields and an index into the table is a whole number of
/// texels.
const TEXEL: usize = 16;

/// How many bytes one row holds.
const ROW: usize = TEXELS_WIDE as usize * TEXEL;

/// A table the shaders read with `textureLoad`, which grows to the largest thing it has held.
///
/// Sized from a high-water mark rather than from each frame's need, so a document that shrinks and
/// grows again does not reallocate; and never sized to nothing, because a bind group has to name a
/// texture whether or not this frame put anything in it.
#[derive(Debug)]
pub struct TableTexture {
    /// The texture.
    texture: wgpu::Texture,
    /// The view a bind group names, kept because the binding borrows it.
    view: wgpu::TextureView,
    /// What it is called, so a driver message names it.
    label: &'static str,
    /// How many rows it holds.
    rows: u32,
    /// Changes whenever `texture` changes identity, for bind-group cache invalidation.
    generation: u64,
}

impl TableTexture {
    /// The smallest allocation, which is also what an empty frame gets.
    const MINIMUM_ROWS: u32 = 1;

    /// An empty table named `label`.
    pub fn new(gpu: &Gpu, label: &'static str) -> Self {
        let (texture, view) = allocate(gpu, label, Self::MINIMUM_ROWS);
        Self {
            texture,
            view,
            label,
            rows: Self::MINIMUM_ROWS,
            generation: 1,
        }
    }

    /// Copies all `values` into the table.
    pub fn upload<T: Pod>(&mut self, gpu: &Gpu, values: &[T]) -> u64 {
        self.upload_range(gpu, values, 0, values.len())
    }

    /// Copies one element range, or the whole slice when growing replaced the texture.
    ///
    /// The unit of a write is a row, so an element range is widened to the rows that hold it. A
    /// table is a few kilobytes and a row is four, so the widening costs less than the arithmetic
    /// to avoid it would.
    pub fn upload_range<T: Pod>(
        &mut self,
        gpu: &Gpu,
        values: &[T],
        start: usize,
        end: usize,
    ) -> u64 {
        let all: &[u8] = bytemuck::cast_slice(values);
        let needed = all.len().div_ceil(ROW) as u32;
        let grew = needed > self.rows;
        if grew {
            self.rows = needed.next_power_of_two().max(Self::MINIMUM_ROWS);
            let (texture, view) = allocate(gpu, self.label, self.rows);
            self.texture = texture;
            self.view = view;
            self.generation = self.generation.wrapping_add(1);
        }
        if all.is_empty() || (start == end && !grew) {
            return 0;
        }

        let element = size_of::<T>();
        let (start, end) = if grew {
            (0, values.len())
        } else {
            (start, end)
        };
        let first = (start * element) / ROW;
        let last = ((end * element).div_ceil(ROW)).max(first + 1);

        // The tail row of a table is rarely full, and a write names whole rows, so the bytes are
        // copied into a row-sized run and the remainder left as whatever the allocation held. What
        // is past the last element is never read: an index past the table is a fault the caller
        // does not make.
        let mut rows = vec![0_u8; (last - first) * ROW];
        let from = first * ROW;
        let to = (last * ROW).min(all.len());
        rows[..to - from].copy_from_slice(&all[from..to]);

        gpu.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: first as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &rows,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW as u32),
                rows_per_image: Some((last - first) as u32),
            },
            wgpu::Extent3d {
                width: TEXELS_WIDE,
                height: (last - first) as u32,
                depth_or_array_layers: 1,
            },
        );

        rows.len() as u64
    }

    /// The binding a bind group names.
    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::TextureView(&self.view)
    }

    /// How many bytes it currently holds.
    pub fn capacity(&self) -> u64 {
        u64::from(self.rows) * ROW as u64
    }

    /// The identity epoch of the allocation a bind group names.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns an oversized high-water allocation to the smallest one.
    pub fn shrink(&mut self, gpu: &Gpu) -> u64 {
        if self.rows <= Self::MINIMUM_ROWS {
            return 0;
        }
        let freed = self.capacity() - u64::from(Self::MINIMUM_ROWS) * ROW as u64;
        let (texture, view) = allocate(gpu, self.label, Self::MINIMUM_ROWS);
        self.texture = texture;
        self.view = view;
        self.rows = Self::MINIMUM_ROWS;
        self.generation = self.generation.wrapping_add(1);
        freed
    }
}

/// Allocates a table of `rows` rows, and the view a bind group names it by.
fn allocate(gpu: &Gpu, label: &'static str, rows: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: TEXELS_WIDE,
            height: rows,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
