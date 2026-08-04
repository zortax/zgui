//! Reusable mapped staging buffers for one frame's host-to-device copies.
//!
//! `Queue::write_buffer` allocates and maps a one-shot staging buffer for every call on native
//! backends. A frame writes a dozen buffers, so an unlucky allocator or mapping round-trip can
//! turn otherwise tiny uploads into a several-millisecond CPU spike. This belt suballocates those
//! writes from a few mapped chunks and recycles them after the submitted copies complete.

use std::sync::mpsc;

use zgui_profile::{Counter, counter};

use crate::gpu::device::Gpu;

/// One mapped staging allocation.
#[derive(Debug)]
struct Chunk {
    buffer: wgpu::Buffer,
    size: u64,
    offset: u64,
    last_used: u64,
}

impl Chunk {
    /// Whether an aligned allocation of `size` fits, and where it would begin.
    fn allocation(&self, size: u64, alignment: u64) -> Option<u64> {
        let offset = self
            .offset
            .next_multiple_of(alignment.max(wgpu::MAP_ALIGNMENT));
        (offset + size <= self.size).then_some(offset)
    }
}

/// A grow-on-demand staging belt which sheds unused high-water chunks after a quiet period.
pub struct UploadBelt {
    active: Vec<Chunk>,
    closed: Vec<Chunk>,
    free: Vec<Chunk>,
    sender: mpsc::Sender<Chunk>,
    receiver: mpsc::Receiver<Chunk>,
    frame: u64,
    mapping_bytes: u64,
    allocations: u32,
}

impl std::fmt::Debug for UploadBelt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UploadBelt")
            .field("active", &self.active.len())
            .field("closed", &self.closed.len())
            .field("free", &self.free.len())
            .field("bytes", &self.bytes())
            .finish_non_exhaustive()
    }
}

impl Default for UploadBelt {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            active: Vec::new(),
            closed: Vec::new(),
            free: Vec::new(),
            sender,
            receiver,
            frame: 0,
            mapping_bytes: 0,
            allocations: 0,
        }
    }
}

impl UploadBelt {
    /// The smallest chunk. Large enough to combine ordinary UI frames into one mapping.
    const MINIMUM: u64 = 256 * 1024;
    /// Free chunks beyond this age are a transient high-water mark rather than working set.
    const RETAIN_FRAMES: u64 = 120;

    /// Starts a frame, opportunistically reclaiming completed mappings without waiting for them.
    pub fn begin_frame(&mut self, gpu: &Gpu) {
        self.frame = self.frame.wrapping_add(1);
        self.allocations = 0;
        let _ = gpu.device().poll(wgpu::PollType::Poll);
        self.receive();

        // Keep at least one chunk warm. Everything else has to have been used recently enough to
        // justify its resident mapped memory.
        let keep = self
            .free
            .iter()
            .enumerate()
            .min_by_key(|(_, chunk)| chunk.size)
            .map(|(index, _)| index);
        let mut index = 0usize;
        self.free.retain(|chunk| {
            let retained = Some(index) == keep
                || self.frame.saturating_sub(chunk.last_used) < Self::RETAIN_FRAMES;
            index += 1;
            retained
        });
    }

    /// Copies bytes into `target` through a mapped reusable chunk.
    pub fn write(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Buffer,
        target_offset: u64,
        bytes: &[u8],
    ) -> u64 {
        if bytes.is_empty() {
            return 0;
        }
        let size = bytes.len() as u64;
        assert!(
            size.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            "upload size {size} is not copy-buffer aligned"
        );
        assert!(
            target_offset.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            "upload offset {target_offset} is not copy-buffer aligned"
        );

        self.receive();
        let active = self
            .active
            .iter()
            .position(|chunk| chunk.allocation(size, wgpu::MAP_ALIGNMENT).is_some());
        let mut chunk = if let Some(index) = active {
            self.active.swap_remove(index)
        } else if let Some(index) = self
            .free
            .iter()
            .position(|chunk| chunk.allocation(size, wgpu::MAP_ALIGNMENT).is_some())
        {
            self.free.swap_remove(index)
        } else {
            self.allocations += 1;
            counter::bump(Counter::UploadChunksAllocated);
            let chunk_size = size.max(Self::MINIMUM).next_power_of_two();
            Chunk {
                buffer: gpu.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("zgui.upload.chunk"),
                    size: chunk_size,
                    usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                }),
                size: chunk_size,
                offset: 0,
                last_used: self.frame,
            }
        };
        let offset = chunk
            .allocation(size, wgpu::MAP_ALIGNMENT)
            .expect("a selected upload chunk has enough room");
        chunk.offset = offset + size;
        chunk.last_used = self.frame;
        self.active.push(chunk);

        let chunk = self.active.last().expect("the selected chunk was retained");
        let slice = chunk.buffer.slice(offset..offset + size);
        encoder.copy_buffer_to_buffer(&chunk.buffer, offset, target, target_offset, size);
        slice.get_mapped_range_mut().copy_from_slice(bytes);

        zgui_profile::counter::add(zgui_profile::Counter::BytesUploaded, size);
        size
    }

    /// Copies tightly packed texels into a texture through padded mapped rows.
    pub fn write_texture(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        bounds: zgui_geom::Rect<i32, zgui_geom::Device>,
        format: zgui_atlas::TextureFormat,
        bytes: &[u8],
    ) -> u64 {
        let width = bounds.size.width.max(0) as u32;
        let height = bounds.size.height.max(0) as u32;
        if width == 0 || height == 0 {
            return 0;
        }
        let row = u64::from(width) * u64::from(format.bytes_per_texel());
        let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64);
        let size = padded * u64::from(height);
        assert_eq!(bytes.len() as u64, row * u64::from(height));

        self.receive();
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64;
        let active = self
            .active
            .iter()
            .position(|chunk| chunk.allocation(size, alignment).is_some());
        let mut chunk = if let Some(index) = active {
            self.active.swap_remove(index)
        } else if let Some(index) = self
            .free
            .iter()
            .position(|chunk| chunk.allocation(size, alignment).is_some())
        {
            self.free.swap_remove(index)
        } else {
            self.allocations += 1;
            counter::bump(Counter::UploadChunksAllocated);
            let chunk_size = size.max(Self::MINIMUM).next_power_of_two();
            Chunk {
                buffer: gpu.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("zgui.texture-upload.chunk"),
                    size: chunk_size,
                    usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                }),
                size: chunk_size,
                offset: 0,
                last_used: self.frame,
            }
        };
        let offset = chunk
            .allocation(size, alignment)
            .expect("a selected texture upload chunk has enough room");
        chunk.offset = offset + size;
        chunk.last_used = self.frame;
        self.active.push(chunk);

        let chunk = self.active.last().expect("the selected chunk was retained");
        let slice = chunk.buffer.slice(offset..offset + size);
        {
            let mut mapped = slice.get_mapped_range_mut();
            for y in 0..height as usize {
                let source = y * row as usize;
                let target = y * padded as usize;
                mapped
                    .slice(target..target + row as usize)
                    .copy_from_slice(&bytes[source..source + row as usize]);
            }
        }
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &chunk.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset,
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: Some(height),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: bounds.origin.x.max(0) as u32,
                    y: bounds.origin.y.max(0) as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        zgui_profile::counter::add(zgui_profile::Counter::BytesUploaded, bytes.len() as u64);
        bytes.len() as u64
    }

    /// Unmaps every chunk referenced by the encoder, making it ready for submission.
    pub fn finish(&mut self) {
        for chunk in self.active.drain(..) {
            chunk.buffer.unmap();
            self.closed.push(chunk);
        }
    }

    /// Schedules submitted chunks to become writable again. This never waits for the GPU.
    pub fn recall(&mut self) {
        self.receive();
        for chunk in self.closed.drain(..) {
            self.mapping_bytes += chunk.size;
            let sender = self.sender.clone();
            chunk
                .buffer
                .clone()
                .slice(..)
                .map_async(wgpu::MapMode::Write, move |result| {
                    if result.is_ok() {
                        let _ = sender.send(chunk);
                    }
                });
        }
    }

    /// Number of chunks allocated during this frame.
    pub fn allocations(&self) -> u32 {
        self.allocations
    }

    /// Mapped and in-flight staging memory held by the belt.
    pub fn bytes(&self) -> u64 {
        self.active
            .iter()
            .chain(&self.closed)
            .chain(&self.free)
            .map(|chunk| chunk.size)
            .sum::<u64>()
            + self.mapping_bytes
    }

    /// Receives only chunks whose mapping callback has already run.
    fn receive(&mut self) {
        while let Ok(mut chunk) = self.receiver.try_recv() {
            self.mapping_bytes = self.mapping_bytes.saturating_sub(chunk.size);
            chunk.offset = 0;
            self.free.push(chunk);
        }
    }
}
