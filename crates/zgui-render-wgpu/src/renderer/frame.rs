//! The buffers, bind groups and counters one frame passes through.

use zgui_scene::Scene;

use crate::bind::globals::Globals;
use crate::bind::tables::Tables;
use crate::buffer::instances::StorageBuffer;
use crate::buffer::slots::SlotBuffer;
use crate::buffer::vectors::VectorInstances;
use crate::gpu::device::Gpu;
use crate::pipeline::kind::PipelineKind;
use crate::pipeline::layout::Layouts;

/// Everything a frame's data is written into.
///
/// The instance buffers are one per pipeline kind rather than one shared buffer, because a batch
/// is a contiguous range of one of the display list's arrays and copying it as bytes is the whole
/// point: a shared buffer would mean either a gather or a per-batch offset, and both cost more
/// than the buffers do.
#[derive(Debug)]
pub struct FrameBuffers {
    /// One block per target the frame draws into.
    pub globals: SlotBuffer,
    /// One block per draw that reads a texture of its own.
    pub blocks: SlotBuffer,
    /// One quad per vector composite this frame draws.
    pub vectors: VectorInstances,
    /// The clip chains.
    pub clips: StorageBuffer,
    /// The paint sources.
    pub paints: StorageBuffer,
    /// Every ramp's stops.
    pub stops: StorageBuffer,
    /// The coordinate systems.
    pub spatial: StorageBuffer,
    /// The rounded rectangles.
    pub quads: StorageBuffer,
    /// The shadows.
    pub shadows: StorageBuffer,
    /// The decoration lines.
    pub decorations: StorageBuffer,
    /// The single-channel coverage sprites.
    pub mono_sprites: StorageBuffer,
    /// The per-channel coverage sprites.
    pub subpixel_sprites: StorageBuffer,
    /// The full-colour sprites.
    pub color_sprites: StorageBuffer,
}

impl FrameBuffers {
    /// Allocates the buffers on `gpu`.
    pub fn new(gpu: &Gpu) -> Self {
        Self {
            globals: SlotBuffer::new::<Globals>(gpu, "zgui.globals"),
            blocks: SlotBuffer::new::<crate::pipeline::composite::CompositeParams>(
                gpu,
                "zgui.blocks",
            ),
            vectors: VectorInstances::new(gpu),
            clips: StorageBuffer::new(gpu, "zgui.clips"),
            paints: StorageBuffer::new(gpu, "zgui.paints"),
            stops: StorageBuffer::new(gpu, "zgui.stops"),
            spatial: StorageBuffer::new(gpu, "zgui.spatial"),
            quads: StorageBuffer::new(gpu, "zgui.quads"),
            shadows: StorageBuffer::new(gpu, "zgui.shadows"),
            decorations: StorageBuffer::new(gpu, "zgui.decorations"),
            mono_sprites: StorageBuffer::new(gpu, "zgui.mono_sprites"),
            subpixel_sprites: StorageBuffer::new(gpu, "zgui.subpixel_sprites"),
            color_sprites: StorageBuffer::new(gpu, "zgui.color_sprites"),
        }
    }

    /// Releases every block staged for the previous frame.
    pub fn begin_frame(&mut self) {
        self.globals.reset();
        self.blocks.reset();
        self.vectors.begin_frame();
    }

    /// Uploads the blocks staged while the frame was planned.
    ///
    /// Every bind group naming them is built afterwards, in the same frame, which is why a buffer
    /// that grew needs no announcement: nothing is holding a name for the one it replaced.
    pub fn upload_blocks(&mut self, gpu: &Gpu) {
        self.globals.upload(gpu);
        self.blocks.upload(gpu);
        self.vectors.upload(gpu);
    }

    /// Writes one frame's side tables and instances, and says how many bytes that was.
    pub fn write(&mut self, gpu: &Gpu, scene: &Scene, tables: &Tables) -> u64 {
        let primitives = &scene.primitives;
        self.clips.write(gpu, &tables.clips)
            + self.paints.write(gpu, &tables.paints)
            + self.stops.write(gpu, &tables.stops)
            + self.spatial.write(gpu, &tables.spatial)
            + self.quads.write(gpu, &primitives.quads)
            + self.shadows.write(gpu, &primitives.shadows)
            + self.decorations.write(gpu, &primitives.decorations)
            + self.mono_sprites.write(gpu, &primitives.mono_sprites)
            + self
                .subpixel_sprites
                .write(gpu, &primitives.subpixel_sprites)
            + self.color_sprites.write(gpu, &primitives.color_sprites)
    }

    /// The bind group one draw reads its own block, texture and sampler through.
    pub fn filtered_bind_group(
        &self,
        gpu: &Gpu,
        layouts: &Layouts,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> Option<wgpu::BindGroup> {
        Some(
            gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("zgui.bind.filtered"),
                layout: &layouts.filtered,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self
                            .blocks
                            .binding::<crate::pipeline::composite::CompositeParams>()?,
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            }),
        )
    }

    /// The bind group a vector composite reads its instances and the scratch through.
    pub fn vector_bind_group(
        &self,
        gpu: &Gpu,
        layouts: &Layouts,
        scratch: &wgpu::TextureView,
    ) -> Option<wgpu::BindGroup> {
        Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zgui.bind.vector"),
            layout: &layouts.vector,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.vectors.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(scratch),
                },
            ],
        }))
    }

    /// The instance buffer a pipeline draws out of.
    pub fn instances(&self, kind: PipelineKind) -> Option<&StorageBuffer> {
        match kind {
            PipelineKind::Quad => Some(&self.quads),
            PipelineKind::Shadow => Some(&self.shadows),
            PipelineKind::Decoration => Some(&self.decorations),
            PipelineKind::MonoSprite => Some(&self.mono_sprites),
            PipelineKind::SubpixelSprite => Some(&self.subpixel_sprites),
            PipelineKind::ColorSprite => Some(&self.color_sprites),
            _ => None,
        }
    }

    /// How many bytes every buffer holds.
    pub fn bytes(&self) -> u64 {
        self.globals.bytes()
            + self.blocks.bytes()
            + self.vectors.bytes()
            + self.clips.capacity()
            + self.paints.capacity()
            + self.stops.capacity()
            + self.spatial.capacity()
            + self.quads.capacity()
            + self.shadows.capacity()
            + self.decorations.capacity()
            + self.mono_sprites.capacity()
            + self.subpixel_sprites.capacity()
            + self.color_sprites.capacity()
    }

    /// The bind group naming the globals and the side tables.
    ///
    /// Built per frame rather than kept, because a buffer that grew this frame is a different
    /// resource and a bind group naming the old one is stale. Seven objects a frame is the cost of
    /// never having to reason about which of them a growth invalidated.
    pub fn frame_bind_group(&self, gpu: &Gpu, layouts: &Layouts) -> Option<wgpu::BindGroup> {
        Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zgui.bind.frame"),
            layout: &layouts.frame,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals.binding::<Globals>()?,
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.clips.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.paints.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.stops.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.spatial.binding(),
                },
            ],
        }))
    }

    /// The bind group naming one pipeline's instances.
    pub fn instance_bind_group(
        &self,
        gpu: &Gpu,
        layouts: &Layouts,
        kind: PipelineKind,
    ) -> Option<wgpu::BindGroup> {
        let instances = self.instances(kind)?;
        Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(kind.label()),
            layout: &layouts.instances,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: instances.binding(),
            }],
        }))
    }
}
