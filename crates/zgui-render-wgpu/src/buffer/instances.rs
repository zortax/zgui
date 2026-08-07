//! The buffers a frame's instances and side tables are written into.

use bytemuck::Pod;

use crate::buffer::upload::UploadBelt;
use crate::gpu::device::Gpu;

/// A storage buffer that grows to the largest thing it has been asked to hold.
///
/// It is sized from a high-water mark rather than from each frame's need, so a scene that shrinks
/// and grows again does not reallocate; and it is never sized to zero, because a bind group has to
/// name a buffer whether or not this frame put anything in it.
#[derive(Debug)]
pub struct StorageBuffer {
    /// The buffer.
    buffer: wgpu::Buffer,
    /// What it is called, so a driver message names it.
    label: &'static str,
    /// How many bytes it holds.
    capacity: u64,
    /// Changes whenever `buffer` changes identity, for bind-group cache invalidation.
    generation: u64,
}

impl StorageBuffer {
    /// The smallest allocation, which is also what an empty frame gets.
    const MINIMUM: u64 = 256;

    /// An empty buffer named `label`.
    pub fn new(gpu: &Gpu, label: &'static str) -> Self {
        Self {
            buffer: allocate(gpu, label, Self::MINIMUM),
            label,
            capacity: Self::MINIMUM,
            generation: 1,
        }
    }

    /// Copies all `values` through the renderer's reusable staging belt.
    pub fn upload<T: Pod>(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        values: &[T],
    ) -> u64 {
        self.upload_range(gpu, belt, encoder, values, 0, values.len())
    }

    /// Copies one element range, or the whole slice when growing replaced the target buffer.
    pub fn upload_range<T: Pod>(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        values: &[T],
        start: usize,
        end: usize,
    ) -> u64 {
        let all = bytemuck::cast_slice(values);
        let needed = all.len() as u64;
        let grew = needed > self.capacity;
        if grew {
            self.capacity = needed.next_power_of_two().max(Self::MINIMUM);
            self.buffer = allocate(gpu, self.label, self.capacity);
            self.generation = self.generation.wrapping_add(1);
        }
        if all.is_empty() || start == end && !grew {
            return 0;
        }
        let element = size_of::<T>();
        let (start, end) = if grew {
            (0, values.len())
        } else {
            (start, end)
        };
        belt.write(
            gpu,
            encoder,
            &self.buffer,
            (start * element) as u64,
            &all[start * element..end * element],
        )
    }

    /// The binding a bind group names.
    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }

    /// How many bytes it currently holds.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// The identity epoch of the allocation a bind group names.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns an oversized high-water allocation to the minimum bindable size.
    pub fn shrink(&mut self, gpu: &Gpu) -> u64 {
        if self.capacity <= Self::MINIMUM {
            return 0;
        }
        let freed = self.capacity - Self::MINIMUM;
        self.buffer = allocate(gpu, self.label, Self::MINIMUM);
        self.capacity = Self::MINIMUM;
        self.generation = self.generation.wrapping_add(1);
        freed
    }
}

/// Allocates a storage buffer of `size` bytes.
fn allocate(gpu: &Gpu, label: &'static str, size: u64) -> wgpu::Buffer {
    gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
