//! The buffers a frame's instances and side tables are written into.

use bytemuck::Pod;
use zgui_profile::{Counter, counter};

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
        }
    }

    /// Writes `values` into the buffer, growing it if they do not fit, and says how many bytes it
    /// wrote.
    ///
    /// The count is returned as well as recorded, because upload volume is a cost that grows with
    /// content and is invisible in a frame time until it is the whole frame time — and a frame's
    /// own figure is not something a process-wide counter can answer.
    pub fn write<T: Pod>(&mut self, gpu: &Gpu, values: &[T]) -> u64 {
        let bytes: &[u8] = bytemuck::cast_slice(values);
        if bytes.len() as u64 > self.capacity {
            // Growth doubles rather than fitting exactly, so a scene that adds one primitive per
            // frame reallocates a logarithmic number of times instead of every frame.
            self.capacity = (bytes.len() as u64).next_power_of_two().max(Self::MINIMUM);
            self.buffer = allocate(gpu, self.label, self.capacity);
        }
        if bytes.is_empty() {
            return 0;
        }
        gpu.queue().write_buffer(&self.buffer, 0, bytes);
        counter::add(Counter::BytesUploaded, bytes.len() as u64);
        bytes.len() as u64
    }

    /// The binding a bind group names.
    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }

    /// How many bytes it currently holds.
    pub fn capacity(&self) -> u64 {
        self.capacity
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
