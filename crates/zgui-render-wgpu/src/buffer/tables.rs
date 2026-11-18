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

use crate::buffer::upload::UploadBelt;
use crate::gpu::device::Gpu;

/// How many texels wide every table is.
///
/// A power of two, so the index a shader holds splits into a column and a row with a mask and a
/// shift rather than a division. It is also what `common.wgsl` and the coverage shader divide by,
/// as `TABLE_TEXELS_WIDE`. A test in each crate compares the shader's number with this one.
pub const TEXELS_WIDE: u32 = 256;

/// How many bytes one texel holds.
///
/// `rgba32uint`: four thirty-two-bit channels. Every structure a table holds is a multiple of four
/// bytes, so a texel is a whole number of fields and an index into the table is a whole number of
/// texels.
pub(crate) const TEXEL: usize = 16;

/// How many bytes one row holds.
pub(crate) const ROW: usize = TEXELS_WIDE as usize * TEXEL;

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
    /// Whether the table was once asked to hold more rows than a texture can, and said so.
    overflowed: bool,
}

impl TableTexture {
    /// The smallest allocation, which is also what an empty frame gets.
    const MINIMUM_ROWS: u32 = 1;

    /// An empty table named `label`.
    pub fn new(gpu: &Gpu, label: &'static str) -> Self {
        let (texture, view) = allocate_rows(gpu, label, Self::MINIMUM_ROWS);
        Self {
            texture,
            view,
            label,
            rows: Self::MINIMUM_ROWS,
            generation: 1,
            overflowed: false,
        }
    }

    /// Copies all `values` into the table.
    pub fn upload<T: Pod>(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        values: &[T],
    ) -> u64 {
        self.upload_range(gpu, belt, encoder, values, 0, values.len())
    }

    /// Copies one element range, or the whole slice when growing replaced the texture.
    ///
    /// The unit of a write is a row, so an element range is widened to the rows that hold it. A
    /// table is a few kilobytes and a row is four, so the widening costs less than the arithmetic
    /// to avoid it would.
    pub fn upload_range<T: Pod>(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        values: &[T],
        start: usize,
        end: usize,
    ) -> u64 {
        let all: &[u8] = bytemuck::cast_slice(values);
        // A texture is at most as tall as the device allows. A table past that keeps the rows that
        // fit, and a record past them reads zeros: `textureLoad` outside a texture is defined.
        let limit = row_limit(gpu);
        let wanted = all.len().div_ceil(ROW);
        let all = if wanted > limit as usize {
            if !std::mem::replace(&mut self.overflowed, true) {
                tracing::error!(
                    table = self.label,
                    rows = wanted,
                    limit,
                    "a table holds more rows than a texture can; the records past the limit are \
                     not drawn"
                );
            }
            &all[..limit as usize * ROW]
        } else {
            all
        };
        let needed = all.len().div_ceil(ROW) as u32;
        let grew = needed > self.rows;
        if grew {
            self.rows = needed
                .next_power_of_two()
                .min(limit)
                .max(Self::MINIMUM_ROWS);
            let (texture, view) = allocate_rows(gpu, self.label, self.rows);
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
        // Cut to the rows the table holds, for a table past the limit.
        let held = all.len().div_ceil(ROW);
        let first = ((start * element) / ROW).min(held - 1);
        let last = ((end * element).div_ceil(ROW)).clamp(first + 1, held);

        // The tail row of a table is rarely full, and a write names whole rows, so the bytes are
        // copied into a row-sized run and the remainder is zero. What is past the last element is
        // never read: an index past the table is a fault the caller does not make.
        //
        // A table that fits in one row is narrowed to the texels it really holds. A copy is
        // rectangular, so only a single row can be shortened without dropping what follows it —
        // and a small table is exactly the case where the whole of it is that tail. A table of
        // four clips cost four kilobytes to write and now costs two hundred and fifty-six bytes.
        let from = first * ROW;
        let to = (last * ROW).min(all.len());
        let wide = if last - first == 1 {
            ((to - from).div_ceil(TEXEL) as u32).min(TEXELS_WIDE)
        } else {
            TEXELS_WIDE
        };
        let mut rows = vec![0_u8; (last - first) * wide as usize * TEXEL];
        rows[..to - from].copy_from_slice(&all[from..to]);

        belt.write_texels(
            gpu,
            encoder,
            &self.texture,
            0,
            (0, first as u32),
            (wide, (last - first) as u32),
            TEXEL as u32,
            &rows,
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
        let (texture, view) = allocate_rows(gpu, self.label, Self::MINIMUM_ROWS);
        self.texture = texture;
        self.view = view;
        self.rows = Self::MINIMUM_ROWS;
        self.generation = self.generation.wrapping_add(1);
        freed
    }
}

/// Writes `bytes` into `texture` as a run of texels starting at `first`.
///
/// A texture write names a rectangle and this data is a line, so a run that crosses a row boundary
/// is up to three of them: the tail of the row it starts in, the whole rows between, and the head
/// of the row it ends in. Widening to whole rows instead would be one write, and it would put
/// whatever this caller does not hold over the elements on either side of the run.
///
/// `bytes` has to be a whole number of texels, which every caller's element size gives it.
pub(crate) fn write_texels(
    gpu: &Gpu,
    belt: &mut UploadBelt,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    first: u32,
    bytes: &[u8],
) {
    debug_assert_eq!(bytes.len() % TEXEL, 0, "a run is a whole number of texels");
    // Cut to what the texture holds. A table at the row limit of the device holds less than it
    // was asked to, and a copy past its last row is one the device refuses.
    let held = u64::from(texture.height()) * u64::from(TEXELS_WIDE);
    let room = held.saturating_sub(u64::from(first)) as usize * TEXEL;
    let mut texel = first;
    let mut rest = &bytes[..bytes.len().min(room)];

    while !rest.is_empty() {
        let column = texel % TEXELS_WIDE;
        let row = texel / TEXELS_WIDE;
        let (width, height) = run_extent(texel, (rest.len() / TEXEL) as u32);
        let taken = width as usize * height as usize * TEXEL;

        belt.write_texels(
            gpu,
            encoder,
            texture,
            0,
            (column, row),
            (width, height),
            TEXEL as u32,
            &rest[..taken],
        );

        texel += width * height;
        rest = &rest[taken..];
    }
}

/// How much of a run starting at `texel` one write can take, as a width and a height in texels.
///
/// Whole rows where the run starts on one and has rows left in it; what remains of the current row
/// otherwise. A run therefore takes at most three writes, whatever its length: a partial row, the
/// whole ones, and a partial row.
fn run_extent(texel: u32, held: u32) -> (u32, u32) {
    let column = texel % TEXELS_WIDE;
    if column == 0 && held >= TEXELS_WIDE {
        (TEXELS_WIDE, held / TEXELS_WIDE)
    } else {
        ((TEXELS_WIDE - column).min(held), 1)
    }
}

/// The most rows a table can have on this device.
pub(crate) fn row_limit(gpu: &Gpu) -> u32 {
    gpu.device().limits().max_texture_dimension_2d
}

/// Allocates a table of `rows` rows, and the view a bind group names it by.
///
/// Every table of this format is allocated here — the side tables, the instance arenas and the
/// frame's vector composites — so that one place states the format, the width and the usage.
pub(crate) fn allocate_rows(
    gpu: &Gpu,
    label: &'static str,
    rows: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
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

#[cfg(test)]
mod tests {
    use super::TEXELS_WIDE;

    #[test]
    fn the_shaders_divide_by_the_width_the_tables_are_allocated_at() {
        let source = zgui_wgsl::module("quad").expect("the quad module exists");
        let declared = format!("const TABLE_TEXELS_WIDE: u32 = {TEXELS_WIDE}u;");
        assert!(
            source.contains(&declared),
            "common.wgsl does not declare `{declared}`"
        );
    }
}
