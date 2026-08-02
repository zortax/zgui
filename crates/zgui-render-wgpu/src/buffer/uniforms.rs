//! The uniform block a frame writes once.

use bytemuck::Pod;

use crate::gpu::device::Gpu;

/// A uniform buffer holding one value.
///
/// Every write to a queue is applied at submission in the order it was made, so a single buffer
/// rewritten between two draws of the same frame does **not** give the two draws different values:
/// both read the last one written. That is why anything that genuinely varies per draw goes
/// somewhere else, and why this holds only what is constant for a whole frame.
#[derive(Debug)]
pub struct UniformBuffer {
    /// The buffer.
    buffer: wgpu::Buffer,
}

impl UniformBuffer {
    /// A buffer able to hold one `T`.
    pub fn new<T: Pod>(gpu: &Gpu, label: &'static str) -> Self {
        Self {
            buffer: gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size_of::<T>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }

    /// Writes `value` into it.
    pub fn write<T: Pod>(&self, gpu: &Gpu, value: &T) {
        gpu.queue()
            .write_buffer(&self.buffer, 0, bytemuck::bytes_of(value));
    }

    /// The binding a bind group names.
    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }
}
