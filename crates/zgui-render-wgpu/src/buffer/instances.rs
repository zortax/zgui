//! The buffers a frame's instances and side tables are written into.

use bytemuck::Pod;

use crate::buffer::tables::{ROW, TEXEL, allocate_rows, write_texels};
use crate::buffer::upload::UploadBelt;
use crate::gpu::device::Gpu;

/// A table of records the shaders read with `textureLoad`, grown to the largest thing it has been
/// asked to hold.
///
/// A texture rather than a storage buffer, for the reason `buffer::tables` states: a GL 3.3
/// context and WebGL 2 have none at all. It is sized from a high-water mark rather than from each
/// frame's need, so a scene that shrinks and grows again does not reallocate; and it is never
/// sized to nothing, because a bind group has to name a texture whether or not this frame put
/// anything in it.
#[derive(Debug)]
pub struct InstanceTable {
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

impl InstanceTable {
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
        }
    }

    /// Copies all `values` through the renderer's reusable staging belt.
    ///
    /// Every record is written at a texel boundary, so its fields sit where a loader can name
    /// them. A record that is already a whole number of texels goes in as it is; a shorter one is
    /// re-spaced on the way, which is what the arenas do for the same reason.
    ///
    /// The whole frame's records go in together, so growing the table throws away what it held
    /// and needs no copy.
    pub fn upload<T: Pod>(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        values: &[T],
    ) -> u64 {
        let element = size_of::<T>();
        let stride = element.div_ceil(TEXEL) * TEXEL;
        let bytes = values.len() * stride;
        let rows = (bytes.div_ceil(ROW) as u32).max(Self::MINIMUM_ROWS);
        if rows > self.rows {
            self.rows = rows.next_power_of_two();
            let (texture, view) = allocate_rows(gpu, self.label, self.rows);
            self.texture = texture;
            self.view = view;
            self.generation = self.generation.wrapping_add(1);
        }
        if values.is_empty() {
            return 0;
        }
        let packed: &[u8] = bytemuck::cast_slice(values);
        if stride == element {
            write_texels(gpu, belt, encoder, &self.texture, 0, packed);
            return packed.len() as u64;
        }
        let mut run = vec![0_u8; bytes];
        for (index, record) in packed.chunks_exact(element).enumerate() {
            run[index * stride..index * stride + element].copy_from_slice(record);
        }
        write_texels(gpu, belt, encoder, &self.texture, 0, &run);
        run.len() as u64
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

    /// Returns an oversized high-water allocation to the minimum bindable size.
    pub fn shrink(&mut self, gpu: &Gpu) -> u64 {
        if self.rows <= Self::MINIMUM_ROWS {
            return 0;
        }
        let freed = (u64::from(self.rows) - u64::from(Self::MINIMUM_ROWS)) * ROW as u64;
        let (texture, view) = allocate_rows(gpu, self.label, Self::MINIMUM_ROWS);
        self.texture = texture;
        self.view = view;
        self.rows = Self::MINIMUM_ROWS;
        self.generation = self.generation.wrapping_add(1);
        freed
    }
}

/// The buffer a lane's draw order is fetched from, as vertex input.
///
/// Vertex input rather than a storage buffer, for the reason the side tables are textures: a GL
/// 3.3 context has no storage buffers at all. The order stream is the ideal thing to feed that
/// way — one entry an instance, already in draw order, and already uploaded every frame — and
/// feeding it takes an indirection out of the vertex stage rather than adding one.
///
/// Seven lanes are fed this way, one entry an instance. The mark lane is fed from a table of its
/// own instead: its instances are its payload's items and its draw names its *position* in the
/// order, so it indexes rather than fetches, and this buffer holds nothing for it.
#[derive(Debug)]
pub struct OrderBuffer {
    /// The buffer.
    buffer: wgpu::Buffer,
    /// What it is called, so a driver message names it.
    label: &'static str,
    /// How many bytes it holds.
    capacity: u64,
    /// Changes whenever `buffer` changes identity, for bind-group cache invalidation.
    generation: u64,
}

impl OrderBuffer {
    /// The smallest allocation, which is also what an empty frame gets.
    const MINIMUM: u64 = 256;

    /// An empty buffer named `label`.
    pub fn new(gpu: &Gpu, label: &'static str) -> Self {
        Self {
            buffer: allocate_order(gpu, label, Self::MINIMUM),
            label,
            capacity: Self::MINIMUM,
            generation: 0,
        }
    }

    /// Copies all `values` through the renderer's reusable staging belt.
    ///
    /// The whole list every time: an order stream is rebuilt from scratch whenever the visible set
    /// or the residence moves, and a frame where neither moved skips the call entirely.
    pub fn upload<T: Pod>(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        values: &[T],
    ) -> u64 {
        let all: &[u8] = bytemuck::cast_slice(values);
        if all.is_empty() {
            return 0;
        }
        let needed = all.len() as u64;
        if needed > self.capacity {
            self.capacity = needed.next_power_of_two().max(Self::MINIMUM);
            self.buffer = allocate_order(gpu, self.label, self.capacity);
            self.generation = self.generation.wrapping_add(1);
        }
        belt.write(gpu, encoder, &self.buffer, 0, all)
    }

    /// The stream from `offset` bytes on, which is where a draw's own range begins.
    ///
    /// A draw binds its range's start rather than asking for a non-zero base instance, which
    /// would need OpenGL 4.2 — and the oldest device this runs on is 3.3.
    pub fn slice_from(&self, offset: u64) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(offset.min(self.capacity)..)
    }

    /// How many bytes it currently holds.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// The identity epoch of the allocation a bind group names.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns an oversized high-water allocation to the minimum.
    pub fn shrink(&mut self, gpu: &Gpu) -> u64 {
        if self.capacity <= Self::MINIMUM {
            return 0;
        }
        let freed = self.capacity - Self::MINIMUM;
        self.buffer = allocate_order(gpu, self.label, Self::MINIMUM);
        self.capacity = Self::MINIMUM;
        self.generation = self.generation.wrapping_add(1);
        freed
    }
}

/// Allocates an order buffer of `size` bytes, readable both ways.
fn allocate_order(gpu: &Gpu, label: &'static str, size: u64) -> wgpu::Buffer {
    gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
