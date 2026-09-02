//! The buffers, bind groups and counters one frame passes through.

use std::cell::RefCell;
use std::collections::HashMap;

use zgui_scene::Scene;

use crate::bind::globals::Globals;
use bytemuck::Pod;

use crate::bind::tables::{DirtySlots, PreparedTables};
use crate::buffer::instances::StorageBuffer;
use crate::buffer::persist::LANES;
use crate::buffer::slots::SlotBuffer;
use crate::buffer::upload::UploadBelt;
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
    /// Reusable CPU-side shader tables and their per-frame dirty slots.
    prepared: PreparedTables,
    /// Mapped staging chunks shared by every upload in a frame.
    uploader: UploadBelt,
    /// One block per target the frame draws into.
    pub globals: SlotBuffer,
    /// One block per draw that reads a texture of its own.
    pub blocks: SlotBuffer,
    /// One block per distinct set of parameters an application effect draws with this frame.
    pub effect_params: SlotBuffer,
    /// Which slot of `effect_params` each interned parameter block was staged into.
    ///
    /// Public so that a plan can borrow it beside the block allocators it also borrows: the two
    /// are disjoint fields, and a method would borrow the whole structure.
    pub effect_offsets: Vec<u32>,
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
    /// The persistent chunk arenas the six instanced pipelines draw out of, and the residence
    /// over them.
    pub chunks: crate::buffer::persist::ChunkStore,
    /// The resolved remap of each instanced kind — packed offset-and-slot entries in draw order.
    pub remaps: [StorageBuffer; LANES.len()],
    /// What each remap buffer holds, so an unchanged frame skips the upload entirely.
    last_remaps: [Vec<u32>; LANES.len()],
    /// The frame's chunk offsets, named by the high bits of a remap entry.
    offsets: StorageBuffer,
    /// What the offsets buffer holds, so a frame that moved nothing skips the upload.
    last_offsets: Vec<[f32; 2]>,
    /// Bind groups whose resources are the stable frame side-table buffers.
    frame_bind: RefCell<Option<([u64; 5], wgpu::BindGroup)>>,
    /// One bind group per lane, keyed by its instance, remap and offset allocation epochs.
    instance_binds: RefCell<HashMap<usize, ([u64; 3], wgpu::BindGroup)>>,
    /// Whether idle trimming replaced the retained side-table buffers with empty allocations.
    tables_released: bool,
}

impl FrameBuffers {
    /// Allocates the buffers on `gpu`.
    pub fn new(gpu: &Gpu) -> Self {
        Self {
            prepared: PreparedTables::default(),
            uploader: UploadBelt::default(),
            globals: SlotBuffer::new::<Globals>(gpu, "zgui.globals"),
            blocks: SlotBuffer::new::<crate::pipeline::composite::CompositeParams>(
                gpu,
                "zgui.blocks",
            ),
            effect_params: SlotBuffer::with_stride(
                gpu,
                "zgui.effect.params",
                zgui_scene::ShaderParams::BYTES as u64,
            ),
            effect_offsets: Vec::new(),
            vectors: VectorInstances::new(gpu),
            clips: StorageBuffer::new(gpu, "zgui.clips"),
            paints: StorageBuffer::new(gpu, "zgui.paints"),
            stops: StorageBuffer::new(gpu, "zgui.stops"),
            spatial: StorageBuffer::new(gpu, "zgui.spatial"),
            chunks: crate::buffer::persist::ChunkStore::new(gpu),
            remaps: [
                StorageBuffer::new(gpu, "zgui.remap.quads"),
                StorageBuffer::new(gpu, "zgui.remap.shadows"),
                StorageBuffer::new(gpu, "zgui.remap.decorations"),
                StorageBuffer::new(gpu, "zgui.remap.mono_sprites"),
                StorageBuffer::new(gpu, "zgui.remap.subpixel_sprites"),
                StorageBuffer::new(gpu, "zgui.remap.color_sprites"),
                StorageBuffer::new(gpu, "zgui.remap.shaded"),
            ],
            last_remaps: Default::default(),
            offsets: StorageBuffer::new(gpu, "zgui.remap.offsets"),
            last_offsets: Vec::new(),
            frame_bind: RefCell::new(None),
            instance_binds: RefCell::new(HashMap::new()),
            tables_released: false,
        }
    }

    /// Releases every block staged for the previous frame and reclaims completed upload chunks.
    pub fn begin_frame(&mut self, gpu: &Gpu) {
        self.uploader.begin_frame(gpu);
        self.globals.reset();
        self.blocks.reset();
        self.effect_params.reset();
        self.effect_offsets.clear();
        self.vectors.begin_frame();
    }

    /// Incrementally prepares the shader side tables while no device work is being recorded.
    pub fn prepare_tables(&mut self, scene: &Scene) {
        self.prepared.update(scene);
    }

    /// Records every frame upload into `encoder`, and says how many bytes will be copied.
    pub fn upload_frame(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
    ) -> u64 {
        let tables = self.prepared.tables();
        let mut uploaded = if self.tables_released {
            self.tables_released = false;
            let mut uploaded = self
                .clips
                .upload(gpu, &mut self.uploader, encoder, &tables.clips);
            uploaded += self
                .paints
                .upload(gpu, &mut self.uploader, encoder, &tables.paints);
            uploaded += self
                .stops
                .upload(gpu, &mut self.uploader, encoder, &tables.stops);
            uploaded += self
                .spatial
                .upload(gpu, &mut self.uploader, encoder, &tables.spatial);
            uploaded
        } else {
            let dirty = self.prepared.dirty();
            let mut uploaded = upload_dirty(
                gpu,
                &mut self.uploader,
                encoder,
                &mut self.clips,
                &tables.clips,
                &dirty.clips,
            );
            uploaded += upload_dirty(
                gpu,
                &mut self.uploader,
                encoder,
                &mut self.paints,
                &tables.paints,
                &dirty.paints,
            );
            uploaded += upload_dirty(
                gpu,
                &mut self.uploader,
                encoder,
                &mut self.stops,
                &tables.stops,
                &dirty.stops,
            );
            uploaded += upload_dirty(
                gpu,
                &mut self.uploader,
                encoder,
                &mut self.spatial,
                &tables.spatial,
                &dirty.spatial,
            );
            uploaded
        };

        // The chunk delta and the frame's transient content go into the persistent arenas; the
        // frame arrays themselves are never uploaded. What each draw reads is the resolved remap
        // — arena slots in draw order — built beside the transient gathering.
        uploaded += self
            .chunks
            .upload_frame(gpu, &mut self.uploader, encoder, scene);
        for (lane, buffer) in self.remaps.iter_mut().enumerate() {
            let resolved = self.chunks.resolved_remap(lane);
            // A frame whose visible set and residence held still resolves to the same list — a
            // blink, an animation elsewhere — and owes the buffer nothing.
            if resolved == self.last_remaps[lane].as_slice() {
                continue;
            }
            uploaded += buffer.upload(gpu, &mut self.uploader, encoder, resolved);
            self.last_remaps[lane].clear();
            self.last_remaps[lane].extend_from_slice(resolved);
        }
        let offsets = self.chunks.frame_offsets();
        if offsets != self.last_offsets.as_slice() {
            uploaded += self
                .offsets
                .upload(gpu, &mut self.uploader, encoder, offsets);
            self.last_offsets.clear();
            self.last_offsets.extend_from_slice(offsets);
        }
        uploaded += self.globals.upload_with(gpu, &mut self.uploader, encoder);
        uploaded += self.blocks.upload_with(gpu, &mut self.uploader, encoder);
        uploaded += self
            .effect_params
            .upload_with(gpu, &mut self.uploader, encoder);
        uploaded += self.vectors.upload_with(gpu, &mut self.uploader, encoder);
        uploaded
    }

    /// Makes all staging chunks readable by the submitted copy commands.
    pub fn finish_uploads(&mut self) {
        self.uploader.finish();
    }

    /// Reclaims staging chunks asynchronously after submission.
    pub fn recall_uploads(&mut self) {
        self.uploader.recall();
    }

    /// Chunks allocated this frame, useful when correlating a rare buffer-growth frame.
    pub fn upload_allocations(&self) -> u32 {
        self.uploader.allocations()
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

    /// Stages one frame's parameter blocks, in the order the scene's table holds them.
    ///
    /// One slot per interned block rather than one per rectangle: two rectangles agreeing about
    /// their parameters are one draw, and interning is what makes them agree.
    pub fn stage_effect_params(&mut self, scene: &Scene) {
        self.effect_offsets.clear();
        for slot in 0..scene.shader_params.slots() {
            let params = scene
                .shader_params
                .get(zgui_scene::ShaderParamsSlot(slot as u32))
                .copied()
                .unwrap_or(zgui_scene::ShaderParams::EMPTY);
            let offset = self.effect_params.stage_bytes(&params.to_bytes());
            self.effect_offsets.push(offset);
        }
    }

    /// The dynamic offset naming the block interned under `slot`.
    pub fn effect_offset(&self, slot: zgui_scene::ShaderParamsSlot) -> Option<u32> {
        self.effect_offsets.get(slot.0 as usize).copied()
    }

    /// The bind group an application effect reads its parameters through.
    pub fn effect_bind_group(&self, gpu: &Gpu, layouts: &Layouts) -> Option<wgpu::BindGroup> {
        Some(
            gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("zgui.bind.effect"),
                layout: &layouts.effect,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self
                        .effect_params
                        .binding_of(zgui_scene::ShaderParams::BYTES as u64)?,
                }],
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

    /// The arena lane a pipeline draws out of.
    pub fn lane(kind: PipelineKind) -> Option<usize> {
        Some(match kind {
            PipelineKind::Quad => 0,
            PipelineKind::Shadow => 1,
            PipelineKind::Decoration => 2,
            PipelineKind::MonoSprite => 3,
            PipelineKind::SubpixelSprite => 4,
            PipelineKind::ColorSprite => 5,
            _ => return None,
        })
    }

    /// The lane application effects draw out of.
    pub const SHADED_LANE: usize = 6;

    /// How many bytes every buffer holds.
    pub fn bytes(&self) -> u64 {
        self.globals.bytes()
            + self.blocks.bytes()
            + self.effect_params.bytes()
            + self.vectors.bytes()
            + self.clips.capacity()
            + self.paints.capacity()
            + self.stops.capacity()
            + self.spatial.capacity()
            + self.chunks.bytes()
            + self.remaps.iter().map(StorageBuffer::capacity).sum::<u64>()
            + self.uploader.bytes()
    }

    /// Shrinks device and host high-water buffers after wall-clock idleness.
    pub fn release_idle(&mut self, gpu: &Gpu) -> u64 {
        let mut freed = self.globals.release() + self.blocks.release();
        freed += self.vectors.shrink(gpu);
        freed += self.clips.shrink(gpu);
        freed += self.paints.shrink(gpu);
        freed += self.stops.shrink(gpu);
        freed += self.spatial.shrink(gpu);
        self.tables_released = true;
        freed += self.chunks.release(gpu);
        for (lane, remap) in self.remaps.iter_mut().enumerate() {
            freed += remap.shrink(gpu);
            self.last_remaps[lane].clear();
        }
        freed += self.uploader.release_idle();
        *self.frame_bind.borrow_mut() = None;
        self.instance_binds.borrow_mut().clear();
        freed
    }

    /// The bind group naming the globals and the side tables.
    ///
    /// Rebuilt only when a backing buffer grows, because a grown buffer is a different
    /// resource and a bind group naming the old one is stale. Seven objects a frame is the cost of
    /// never having to reason about which of them a growth invalidated.
    pub fn frame_bind_group(&self, gpu: &Gpu, layouts: &Layouts) -> Option<wgpu::BindGroup> {
        let signature = [
            self.globals.generation(),
            self.clips.generation(),
            self.paints.generation(),
            self.stops.generation(),
            self.spatial.generation(),
        ];
        if let Some((held, bind)) = self.frame_bind.borrow().as_ref()
            && *held == signature
        {
            return Some(bind.clone());
        }
        let bind = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
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
        });
        *self.frame_bind.borrow_mut() = Some((signature, bind.clone()));
        Some(bind)
    }

    /// The bind group naming one lane's instances.
    ///
    /// Keyed by the lane rather than by the pipeline drawing it, because an application effect
    /// draws out of a lane through a pipeline this crate never enumerated.
    pub fn instance_bind_group(
        &self,
        gpu: &Gpu,
        layouts: &Layouts,
        lane: usize,
    ) -> Option<wgpu::BindGroup> {
        let remap = self.remaps.get(lane)?;
        let signature = [
            self.chunks.generation(lane),
            remap.generation(),
            self.offsets.generation(),
        ];
        if let Some((held, bind)) = self.instance_binds.borrow().get(&lane)
            && *held == signature
        {
            return Some(bind.clone());
        }
        let bind = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(lane_label(lane)),
            layout: &layouts.instances,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.chunks.binding(lane),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: remap.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.offsets.binding(),
                },
            ],
        });
        self.instance_binds
            .borrow_mut()
            .insert(lane, (signature, bind.clone()));
        Some(bind)
    }
}

/// A label for one lane's instance bind group, so a driver error names which lane it came from.
fn lane_label(lane: usize) -> &'static str {
    match LANES.get(lane) {
        Some(zgui_scene::PrimitiveKind::Quad) => "zgui.bind.instances.quads",
        Some(zgui_scene::PrimitiveKind::Shadow) => "zgui.bind.instances.shadows",
        Some(zgui_scene::PrimitiveKind::Decoration) => "zgui.bind.instances.decorations",
        Some(zgui_scene::PrimitiveKind::MonoSprite) => "zgui.bind.instances.mono_sprites",
        Some(zgui_scene::PrimitiveKind::SubpixelSprite) => "zgui.bind.instances.subpixel_sprites",
        Some(zgui_scene::PrimitiveKind::ColorSprite) => "zgui.bind.instances.color_sprites",
        Some(zgui_scene::PrimitiveKind::Shaded) => "zgui.bind.instances.shaded",
        _ => "zgui.bind.instances",
    }
}

/// Uploads dirty slots as coalesced ranges. A half-dirty table is cheaper as one full copy.
fn upload_dirty<T: Pod>(
    gpu: &Gpu,
    belt: &mut UploadBelt,
    encoder: &mut wgpu::CommandEncoder,
    buffer: &mut StorageBuffer,
    values: &[T],
    dirty: &DirtySlots,
) -> u64 {
    const MAX_RANGES: usize = 16;
    let ranges = dirty
        .slots
        .windows(2)
        .filter(|pair| pair[1] != pair[0] + 1)
        .count()
        + usize::from(!dirty.slots.is_empty());
    if dirty.all || dirty.slots.len().saturating_mul(2) >= values.len() || ranges > MAX_RANGES {
        return buffer.upload(gpu, belt, encoder, values);
    }
    let mut uploaded = 0;
    let mut slots = dirty.slots.iter().copied().peekable();
    while let Some(first) = slots.next() {
        let mut end = first + 1;
        while slots.next_if_eq(&end).is_some() {
            end += 1;
        }
        uploaded += buffer.upload_range(gpu, belt, encoder, values, first as usize, end as usize);
    }
    uploaded
}
