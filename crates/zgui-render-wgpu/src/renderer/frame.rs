//! The buffers, bind groups and counters one frame passes through.

use std::cell::RefCell;

use zgui_scene::Scene;

use crate::bind::globals::Globals;
use bytemuck::Pod;

use crate::bind::tables::{DirtySlots, PreparedTables};
use crate::buffer::instances::OrderBuffer;
use crate::buffer::persist::OrderEntry;
use crate::buffer::persist::{LANES, PAYLOAD_LANES};
use crate::buffer::slots::SlotBuffer;
use crate::buffer::tables::TableTexture;
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
    /// One block per mark item a draw of this frame names.
    pub mark_draws: SlotBuffer,
    /// Which slot of `effect_params` each interned parameter block was staged into.
    ///
    /// Public so that a plan can borrow it beside the block allocators it also borrows: the two
    /// are disjoint fields, and a method would borrow the whole structure.
    pub effect_offsets: Vec<u32>,
    /// One quad per vector composite this frame draws.
    pub vectors: VectorInstances,
    /// The clip chains.
    pub clips: TableTexture,
    /// The paint sources.
    pub paints: TableTexture,
    /// Every ramp's stops.
    pub stops: TableTexture,
    /// The coordinate systems.
    pub spatial: TableTexture,
    /// The persistent chunk arenas the six instanced pipelines draw out of, and the residence
    /// over them.
    pub chunks: crate::buffer::persist::ChunkStore,
    /// The resolved draw order of each instanced kind, as the vertex stream carries it.
    pub orders: [OrderBuffer; LANES.len()],
    /// What each order buffer holds, so an unchanged frame skips the upload entirely.
    last_orders: [Vec<OrderEntry>; LANES.len()],
    /// Bind groups whose resources are the stable frame side-table buffers.
    frame_bind: RefCell<Option<([u64; 5], wgpu::BindGroup)>>,
    /// One bind group per lane, keyed by its arena's and its order's allocation epochs.
    ///
    /// Two epochs, where there were three: the chunk offsets are resolved into the order stream
    /// on the host, so nothing binds them any more.
    instance_binds: RefCell<[Keyed<2>; LANES.len()]>,
    /// One bind group per mark payload kind, keyed by its block and payload allocation epochs.
    mark_binds: RefCell<[Keyed<2>; PAYLOAD_LANES]>,
    /// The bind groups draws read a block, a texture and a sampler through.
    filtered_binds: RefCell<ViewBinds>,
    /// The bind groups vector composites read their instances and a scratch through.
    vector_binds: RefCell<ViewBinds>,
    /// The bind group application effects read their parameters through, keyed by the
    /// allocation epoch of the parameter buffer.
    effect_bind: RefCell<Option<(u64, wgpu::BindGroup)>>,
    /// The bind group mark composites read their bins through, keyed by the block and the pages'
    /// allocation epochs.
    mark_composite_bind: RefCell<Option<([u64; 2], wgpu::BindGroup)>>,
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
            mark_draws: SlotBuffer::new::<crate::pipeline::marks::MarkDraw>(gpu, "zgui.mark_draws"),
            vectors: VectorInstances::new(gpu),
            clips: TableTexture::new(gpu, "zgui.clips"),
            paints: TableTexture::new(gpu, "zgui.paints"),
            stops: TableTexture::new(gpu, "zgui.stops"),
            spatial: TableTexture::new(gpu, "zgui.spatial"),
            chunks: crate::buffer::persist::ChunkStore::new(gpu),
            orders: [
                OrderBuffer::new(gpu, "zgui.order.quads"),
                OrderBuffer::new(gpu, "zgui.order.shadows"),
                OrderBuffer::new(gpu, "zgui.order.decorations"),
                OrderBuffer::new(gpu, "zgui.order.mono_sprites"),
                OrderBuffer::new(gpu, "zgui.order.subpixel_sprites"),
                OrderBuffer::new(gpu, "zgui.order.color_sprites"),
                OrderBuffer::new(gpu, "zgui.order.shaded"),
                OrderBuffer::new(gpu, "zgui.order.marks"),
            ],
            last_orders: Default::default(),
            frame_bind: RefCell::new(None),
            instance_binds: RefCell::default(),
            mark_binds: RefCell::default(),
            filtered_binds: RefCell::default(),
            vector_binds: RefCell::default(),
            effect_bind: RefCell::new(None),
            mark_composite_bind: RefCell::new(None),
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
        self.mark_draws.reset();
        self.vectors.begin_frame();
        self.filtered_binds.get_mut().begin_frame();
        self.vector_binds.get_mut().begin_frame();
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
            let mut uploaded = self.clips.upload(gpu, &tables.clips);
            uploaded += self.paints.upload(gpu, &tables.paints);
            uploaded += self.stops.upload(gpu, &tables.stops);
            uploaded += self.spatial.upload(gpu, &tables.spatial);
            uploaded
        } else {
            let dirty = self.prepared.dirty();
            let mut uploaded = upload_dirty(gpu, &mut self.clips, &tables.clips, &dirty.clips);
            uploaded += upload_dirty(gpu, &mut self.paints, &tables.paints, &dirty.paints);
            uploaded += upload_dirty(gpu, &mut self.stops, &tables.stops, &dirty.stops);
            uploaded += upload_dirty(gpu, &mut self.spatial, &tables.spatial, &dirty.spatial);
            uploaded
        };

        // The chunk delta and the frame's transient content go into the persistent arenas; the
        // frame arrays themselves are never uploaded. What each draw reads is the resolved remap
        // — arena slots in draw order — built beside the transient gathering.
        uploaded += self
            .chunks
            .upload_frame(gpu, &mut self.uploader, encoder, scene);
        for (lane, buffer) in self.orders.iter_mut().enumerate() {
            let resolved = self.chunks.resolved_order(lane);
            // A frame whose visible set and residence held still resolves to the same list — a
            // blink, an animation elsewhere — and owes the buffer nothing.
            if resolved == self.last_orders[lane].as_slice() {
                continue;
            }
            uploaded += buffer.upload(gpu, &mut self.uploader, encoder, resolved);
            self.last_orders[lane].clear();
            self.last_orders[lane].extend_from_slice(resolved);
        }
        uploaded += self.globals.upload_with(gpu, &mut self.uploader, encoder);
        uploaded += self.blocks.upload_with(gpu, &mut self.uploader, encoder);
        uploaded += self
            .effect_params
            .upload_with(gpu, &mut self.uploader, encoder);
        uploaded += self
            .mark_draws
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
    ///
    /// Kept while a frame binds it, and keyed by the allocation epoch of the blocks, the view and
    /// the sampler. A composite reads the same pool target from one frame to the next, and making
    /// its bind group again on each draw costs a device call.
    pub fn filtered_bind_group(
        &self,
        gpu: &Gpu,
        layouts: &Layouts,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> Option<wgpu::BindGroup> {
        let generation = self.blocks.generation();
        let mut binds = self.filtered_binds.borrow_mut();
        if let Some(bind) = binds.find(generation, view, Some(sampler)) {
            return Some(bind);
        }
        let bind = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
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
        });
        binds.keep(generation, view, Some(sampler), &bind);
        Some(bind)
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
        let generation = self.effect_params.generation();
        if let Some((held, bind)) = self.effect_bind.borrow().as_ref()
            && *held == generation
        {
            return Some(bind.clone());
        }
        let bind = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zgui.bind.effect"),
            layout: &layouts.effect,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self
                    .effect_params
                    .binding_of(zgui_scene::ShaderParams::BYTES as u64)?,
            }],
        });
        *self.effect_bind.borrow_mut() = Some((generation, bind.clone()));
        Some(bind)
    }

    /// The bind group a vector composite reads its instances and the scratch through.
    pub fn vector_bind_group(
        &self,
        gpu: &Gpu,
        layouts: &Layouts,
        scratch: &wgpu::TextureView,
    ) -> Option<wgpu::BindGroup> {
        let generation = self.vectors.generation();
        let mut binds = self.vector_binds.borrow_mut();
        if let Some(bind) = binds.find(generation, scratch, None) {
            return Some(bind);
        }
        let bind = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
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
        });
        binds.keep(generation, scratch, None, &bind);
        Some(bind)
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

    /// The lane mark items are drawn out of.
    pub const MARKS_LANE: usize = crate::buffer::persist::MARKS_LANE;

    /// The bind group a mark draw reads its block and the payload of `kind` through.
    pub fn mark_bind_group(
        &self,
        gpu: &Gpu,
        layouts: &Layouts,
        kind: crate::pipeline::marks::MarkKind,
    ) -> Option<wgpu::BindGroup> {
        let lane = kind.lane();
        let signature = [
            self.mark_draws.generation(),
            self.chunks.payload_generation(lane),
        ];
        if let Some((held, bind)) = self.mark_binds.borrow().get(lane)?
            && *held == signature
        {
            return Some(bind.clone());
        }
        let bind = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zgui.bind.marks"),
            layout: &layouts.marks,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self
                        .mark_draws
                        .binding::<crate::pipeline::marks::MarkDraw>()?,
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.chunks.payload_binding(lane),
                },
            ],
        });
        *self.mark_binds.borrow_mut().get_mut(lane)? = Some((signature, bind.clone()));
        Some(bind)
    }

    /// The bind group a mark composite reads its block and the bin pages through.
    pub fn mark_composite_bind_group(
        &self,
        gpu: &Gpu,
        layouts: &Layouts,
        bins: &wgpu::TextureView,
        bins_generation: u64,
    ) -> Option<wgpu::BindGroup> {
        let signature = [self.mark_draws.generation(), bins_generation];
        if let Some((held, bind)) = self.mark_composite_bind.borrow().as_ref()
            && *held == signature
        {
            return Some(bind.clone());
        }
        let bind = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zgui.bind.mark_composite"),
            layout: &layouts.mark_composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self
                        .mark_draws
                        .binding::<crate::pipeline::marks::MarkDraw>()?,
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(bins),
                },
            ],
        });
        *self.mark_composite_bind.borrow_mut() = Some((signature, bind.clone()));
        Some(bind)
    }

    /// How many bytes every buffer holds.
    pub fn bytes(&self) -> u64 {
        self.globals.bytes()
            + self.blocks.bytes()
            + self.mark_draws.bytes()
            + self.effect_params.bytes()
            + self.vectors.bytes()
            + self.clips.capacity()
            + self.paints.capacity()
            + self.stops.capacity()
            + self.spatial.capacity()
            + self.chunks.bytes()
            + self.orders.iter().map(OrderBuffer::capacity).sum::<u64>()
            + self.uploader.bytes()
    }

    /// Shrinks device and host high-water buffers after wall-clock idleness.
    pub fn release_idle(&mut self, gpu: &Gpu) -> u64 {
        let mut freed = self.globals.release() + self.blocks.release() + self.mark_draws.release();
        freed += self.vectors.shrink(gpu);
        freed += self.clips.shrink(gpu);
        freed += self.paints.shrink(gpu);
        freed += self.stops.shrink(gpu);
        freed += self.spatial.shrink(gpu);
        self.tables_released = true;
        freed += self.chunks.release(gpu);
        for (lane, order) in self.orders.iter_mut().enumerate() {
            freed += order.shrink(gpu);
            self.last_orders[lane].clear();
        }
        freed += self.uploader.release_idle();
        *self.frame_bind.borrow_mut() = None;
        *self.instance_binds.borrow_mut() = Default::default();
        *self.mark_binds.borrow_mut() = Default::default();
        *self.mark_composite_bind.borrow_mut() = None;
        self.forget_view_binds();
        *self.effect_bind.get_mut() = None;
        freed
    }

    /// Drops every kept bind group that names a texture view, so that no view keeps a target alive.
    pub fn forget_view_binds(&mut self) {
        self.filtered_binds.get_mut().clear();
        self.vector_binds.get_mut().clear();
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
        let order = self.orders.get(lane)?;
        let signature = [self.chunks.generation(lane), order.generation()];
        if let Some((held, bind)) = self.instance_binds.borrow().get(lane)?
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
                    resource: order.binding(),
                },
            ],
        });
        *self.instance_binds.borrow_mut().get_mut(lane)? = Some((signature, bind.clone()));
        Some(bind)
    }
}

/// A bind group and the allocation epochs of the `N` resources it names.
type Keyed<const N: usize> = Option<([u64; N], wgpu::BindGroup)>;

/// Bind groups that name a texture view, each kept while a frame binds it.
///
/// A bind group holds its view, and through it the texture. One that no draw of the frame before
/// bound is dropped at the start of the next, so a target the pool gave back is held for no more
/// than one frame past its last use.
#[derive(Debug, Default)]
struct ViewBinds {
    /// The bind groups, in the order they were made.
    entries: Vec<ViewBind>,
    /// The frame being drawn, counted by [`ViewBinds::begin_frame`].
    frame: u64,
}

/// One bind group that names a texture view.
#[derive(Debug)]
struct ViewBind {
    /// The allocation epoch of the buffer it names.
    generation: u64,
    /// The view it names, held so that no other view can take its identity while it is kept.
    view: wgpu::TextureView,
    /// The sampler it names, if any.
    sampler: Option<wgpu::Sampler>,
    /// The bind group.
    bind: wgpu::BindGroup,
    /// The last frame that bound it.
    used: u64,
}

impl ViewBinds {
    /// Drops every bind group the frame before did not bind, and starts the next frame.
    fn begin_frame(&mut self) {
        let frame = self.frame;
        self.entries.retain(|entry| entry.used == frame);
        self.frame = frame.wrapping_add(1);
    }

    /// The kept bind group naming the buffer at `generation`, `view` and `sampler`.
    fn find(
        &mut self,
        generation: u64,
        view: &wgpu::TextureView,
        sampler: Option<&wgpu::Sampler>,
    ) -> Option<wgpu::BindGroup> {
        let frame = self.frame;
        let entry = self.entries.iter_mut().find(|entry| {
            entry.generation == generation
                && entry.view == *view
                && entry.sampler.as_ref() == sampler
        })?;
        entry.used = frame;
        Some(entry.bind.clone())
    }

    /// Keeps `bind`, which names the buffer at `generation`, `view` and `sampler`.
    fn keep(
        &mut self,
        generation: u64,
        view: &wgpu::TextureView,
        sampler: Option<&wgpu::Sampler>,
        bind: &wgpu::BindGroup,
    ) {
        // A bind group naming a buffer that has since been replaced can never be found again.
        self.entries.retain(|entry| entry.generation == generation);
        self.entries.push(ViewBind {
            generation,
            view: view.clone(),
            sampler: sampler.cloned(),
            bind: bind.clone(),
            used: self.frame,
        });
    }

    /// Drops every bind group.
    fn clear(&mut self) {
        self.entries.clear();
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
        Some(zgui_scene::PrimitiveKind::Marks) => "zgui.bind.instances.marks",
        _ => "zgui.bind.instances",
    }
}

/// Uploads dirty slots as coalesced ranges. A half-dirty table is cheaper as one full copy.
/// A table is a texture, so the write needs no belt and no encoder.
fn upload_dirty<T: Pod>(
    gpu: &Gpu,
    table: &mut TableTexture,
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
        return table.upload(gpu, values);
    }
    let mut uploaded = 0;
    let mut slots = dirty.slots.iter().copied().peekable();
    while let Some(first) = slots.next() {
        let mut end = first + 1;
        while slots.next_if_eq(&end).is_some() {
            end += 1;
        }
        uploaded += table.upload_range(gpu, values, first as usize, end as usize);
    }
    uploaded
}

#[cfg(test)]
mod tests {
    use zgui_bits::DamageSet;
    use zgui_color::Color;
    use zgui_geom::{Device, DevicePx, Point, Rect, Scale, Size};
    use zgui_render::{RenderTarget, Renderer as _};
    use zgui_scene::{Filter, GroupBoundary, Paint, Quad, Scene};

    use crate::Builder;

    /// A rectangle in device pixels.
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    /// A white background under a blurred, half-transparent group.
    fn blurred_group() -> Scene {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(64, 64));
        let white = scene
            .paints
            .add(Paint::Solid(Color::srgb_u8(255, 255, 255, 255)));
        let black = scene.paints.add(Paint::Solid(Color::srgb_u8(0, 0, 0, 255)));
        scene.push_quad(Quad::filled(rect(0.0, 0.0, 64.0, 64.0), white));
        let boundary = GroupBoundary::start(
            rect(8.0, 8.0, 48.0, 48.0),
            0.5,
            zgui_scene::peniko::BlendMode::default(),
            [Filter::Blur(2.0)].into_iter().collect(),
        );
        scene.push_group(boundary.clone());
        scene.push_quad(Quad::filled(rect(16.0, 16.0, 32.0, 32.0), black));
        scene.push_group(boundary.end());
        scene.finish(&DamageSet::full());
        scene
    }

    #[test]
    fn a_frame_drawn_again_binds_its_filtered_targets_through_the_kept_bind_groups() {
        let target = RenderTarget::new(Size::new(64, 64), Scale::new(1.0));
        let mut renderer =
            match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
                Ok(renderer) => renderer,
                Err(failure) => {
                    eprintln!("skipped: no usable graphics device ({failure})");
                    return;
                }
            };
        let scene = blurred_group();
        let kept = |renderer: &crate::WgpuRenderer| -> Vec<wgpu::BindGroup> {
            let binds = renderer.buffers.filtered_binds.borrow();
            binds
                .entries
                .iter()
                .map(|entry| entry.bind.clone())
                .collect()
        };

        let _ = renderer.draw(&scene, &DamageSet::full());
        let first = kept(&renderer);
        assert!(!first.is_empty(), "a blurred group binds a filtered target");
        let _ = renderer.draw(&scene, &DamageSet::full());
        assert_eq!(
            kept(&renderer),
            first,
            "the second frame makes no filtered bind group of its own"
        );

        renderer.buffers.forget_view_binds();
        assert!(
            kept(&renderer).is_empty(),
            "and none is kept once forgotten"
        );
    }

    #[test]
    fn a_pipeline_a_frame_builds_is_written_to_the_cache_after_the_frame() {
        let target = RenderTarget::new(Size::new(64, 64), Scale::new(1.0));
        let mut renderer =
            match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
                Ok(renderer) => renderer,
                Err(failure) => {
                    eprintln!("skipped: no usable graphics device ({failure})");
                    return;
                }
            };
        if !renderer.pipelines.borrow().cache().persists() {
            eprintln!("skipped: the device keeps no pipeline cache, or there is no path for it");
            return;
        }
        let unsaved =
            |renderer: &crate::WgpuRenderer| renderer.pipelines.borrow().cache().unsaved();
        assert!(
            !unsaved(&renderer),
            "the pipelines built at start are written"
        );

        // The blurred group is drawn into a target of the group format, whose pipelines no
        // renderer builds before a frame asks for them.
        let _ = renderer.draw(&blurred_group(), &DamageSet::full());
        assert!(
            !unsaved(&renderer),
            "the pipelines the frame built are written after it"
        );

        let gpu = std::sync::Arc::clone(&renderer.gpu);
        let format = crate::target::group_pool::GroupPool::FORMAT;
        let _ = renderer.pipelines.borrow_mut().get(
            &gpu,
            crate::pipeline::kind::PipelineKind::Quad,
            format,
        );
        assert!(
            !unsaved(&renderer),
            "the frame built the group format's quad pipeline, so asking again builds nothing"
        );
        let _ = renderer.pipelines.borrow_mut().get(
            &gpu,
            crate::pipeline::kind::PipelineKind::Quad,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        assert!(
            unsaved(&renderer),
            "a pipeline built for a new format is unsaved until the next write"
        );
    }
}
