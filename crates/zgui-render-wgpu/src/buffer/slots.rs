//! A uniform buffer of independently addressable slots.

use bytemuck::Pod;

use crate::buffer::upload::UploadBelt;
use crate::gpu::device::Gpu;

/// A uniform buffer holding many values, each addressed by a dynamic offset.
///
/// Writes to a queue are applied at submission in the order they were made, so rewriting one
/// buffer between two draws does not give the two draws different values — both read the last
/// write. Anything that varies within a frame therefore takes a slot of its own and is bound
/// through a dynamic offset, which is the only arrangement in which two draws of one frame read
/// two different blocks of the same buffer.
///
/// Slots are staged on the host and uploaded once, rather than written straight through. That is
/// what makes growth safe: a frame that needs more slots than the last one reallocates, and a
/// reallocation part-way through a frame would otherwise discard the slots already written into
/// the buffer it replaced.
#[derive(Debug)]
pub struct SlotBuffer {
    /// The buffer, or `None` before anything was ever uploaded.
    buffer: Option<wgpu::Buffer>,
    /// A label, so a driver error names which allocator it came from.
    label: &'static str,
    /// The stride between slots, which is the device's own minimum alignment.
    stride: u64,
    /// This frame's slots, padded to the stride.
    staged: Vec<u8>,
    /// How many slots have been staged this frame.
    taken: u32,
    /// Changes whenever `buffer` changes identity.
    generation: u64,
}

impl SlotBuffer {
    /// An allocator over blocks of `T`, aligned as `gpu` requires.
    pub fn new<T: Pod>(gpu: &Gpu, label: &'static str) -> Self {
        let alignment = u64::from(gpu.device().limits().min_uniform_buffer_offset_alignment);
        let size = size_of::<T>() as u64;
        Self {
            buffer: None,
            label,
            stride: size.next_multiple_of(alignment.max(1)),
            staged: Vec::new(),
            taken: 0,
            generation: 0,
        }
    }

    /// Releases every slot, so the next frame starts from the beginning of the buffer.
    pub fn reset(&mut self) {
        self.staged.clear();
        self.taken = 0;
    }

    /// Stages `value` in a fresh slot and returns the dynamic offset that will name it.
    ///
    /// The offset is valid once [`SlotBuffer::upload_with`] has run, which is why the two are separate:
    /// a frame plans every block it needs, uploads them together, and only then builds the bind
    /// group that reads them.
    pub fn stage<T: Pod>(&mut self, value: &T) -> u32 {
        let offset = u64::from(self.taken) * self.stride;
        self.staged.resize(offset as usize, 0);
        self.staged.extend_from_slice(bytemuck::bytes_of(value));
        self.staged.resize((offset + self.stride) as usize, 0);
        self.taken += 1;
        offset as u32
    }

    /// Uploads everything staged through a reusable mapped belt.
    pub fn upload_with(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
    ) -> u64 {
        if self.staged.is_empty() {
            return 0;
        }
        let needed = self.staged.len() as u64;
        let outgrown = match &self.buffer {
            Some(buffer) => buffer.size() < needed,
            None => true,
        };
        if outgrown {
            self.buffer = Some(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: needed.next_power_of_two(),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.generation = self.generation.wrapping_add(1);
        }
        belt.write(
            gpu,
            encoder,
            self.buffer
                .as_ref()
                .expect("a non-empty upload has allocated a buffer"),
            0,
            &self.staged,
        )
    }

    /// The binding a bind group names: one slot's worth of the buffer, moved by a dynamic offset.
    ///
    /// `None` before anything has ever been uploaded, when there is no buffer to name.
    pub fn binding<T: Pod>(&self) -> Option<wgpu::BindingResource<'_>> {
        let buffer = self.buffer.as_ref()?;
        Some(wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer,
            offset: 0,
            size: wgpu::BufferSize::new(size_of::<T>() as u64),
        }))
    }

    /// The stride between two slots.
    pub fn stride(&self) -> u64 {
        self.stride
    }

    /// How many bytes are allocated on the device.
    pub fn bytes(&self) -> u64 {
        self.buffer.as_ref().map_or(0, wgpu::Buffer::size)
    }

    /// The identity epoch of the allocation a bind group names.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Drops the high-water allocation and host staging capacity while idle.
    pub fn release(&mut self) -> u64 {
        let freed = self.bytes();
        if self.buffer.take().is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
        self.staged.clear();
        self.staged.shrink_to_fit();
        freed
    }

    /// How many slots this frame has staged.
    pub fn taken(&self) -> u32 {
        self.taken
    }
}
