//! Recording a planned frame into one command encoder.

use std::collections::BTreeMap;

use zgui_geom::{Device, Rect, Size};
use zgui_profile::{Counter, counter};
use zgui_scene::{Batch, ExternalTextureId};

use crate::atlas_backend::sink::AtlasTextures;
use crate::frame::build::FramePlan;
use crate::frame::segment::{EncoderOp, PassLoad, PlannedDraw, PlannedPass, Segment};
use crate::frame::target::TargetRef;
use crate::gpu::device::Gpu;
use crate::gpu::timing::{GpuTimer, PassQueries, TimedWork};
use crate::pipeline::Pipelines;
use crate::pipeline::kind::PipelineKind;
use crate::renderer::frame::FrameBuffers;
use crate::target::group_pool::GroupPool;
use crate::target::scene_texture::SceneTexture;

/// What one frame's recording produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Recorded {
    /// How many draw calls were issued.
    pub draw_calls: u32,
    /// How many planned draws could not be issued, for want of a pipeline or a texture.
    pub dropped: u32,
}

impl core::ops::AddAssign for Recorded {
    fn add_assign(&mut self, other: Self) {
        self.draw_calls += other.draw_calls;
        self.dropped += other.dropped;
    }
}

/// A texture the renderer was handed rather than one it drew.
pub struct AttachedTexture {
    /// What it is.
    pub texture: zgui_render::ExternalTexture,
    /// A view of it.
    pub view: wgpu::TextureView,
}

/// Everything a planned frame is recorded against.
pub struct Recorder<'frame> {
    /// The device.
    pub gpu: &'frame Gpu,
    /// The pipelines, built on demand.
    pub pipelines: &'frame mut Pipelines,
    /// The frame's buffers.
    pub buffers: &'frame FrameBuffers,
    /// The atlas textures.
    pub atlas: &'frame AtlasTextures,
    /// The pool isolated targets were lent by.
    pub pool: &'frame GroupPool,
    /// The persistent target the frame composes into.
    pub composed: &'frame SceneTexture,
    /// The persistent copy a backdrop filter reads what lies beneath from.
    ///
    /// Absent until a frame holds a backdrop that keeps its capture, which is most frames of most
    /// documents: nothing allocates it and no pass names it.
    pub kept: Option<&'frame SceneTexture>,
    /// The sampler every magnifying read goes through.
    pub sampler: &'frame wgpu::Sampler,
    /// Textures the renderer did not draw.
    pub externals: &'frame BTreeMap<ExternalTextureId, AttachedTexture>,
    /// Whatever rasterised this frame's vector content, when there is one.
    pub vectors: Option<&'frame dyn crate::frame::vector::VectorSource>,
    /// The bin pages union marks write their coverage into.
    pub marks: &'frame crate::target::marks::MarksScratch,
    /// The device extent of one bin page, which a page pass's block describes.
    pub mark_extent: Size<i32, Device>,
    /// The frame's pass timing, when timing is on.
    pub(crate) timer: Option<&'frame mut GpuTimer>,
}

impl Recorder<'_> {
    /// Records `segments` of `plan` into `encoder`.
    ///
    /// One pass is opened per pass segment and dropped before the next segment, because a live
    /// pass holds the encoder borrowed and the operations between passes need it. That constraint
    /// is not worked around here — it is what the plan is a plan *of*.
    ///
    /// A vector round records nothing: the caller cuts the plan at each one with
    /// [`FramePlan::runs`], because a round is rasterised between two submissions.
    pub fn record(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        plan: &FramePlan,
        segments: &[Segment],
    ) -> Recorded {
        let mut recorded = Recorded::default();
        let mut index = 0;
        while index < segments.len() {
            match &segments[index] {
                Segment::Encoder(op) => {
                    self.run(encoder, *op);
                    index += 1;
                }
                Segment::Pass(first) => {
                    // A damage rectangle is planned as a pass of its own, and on a frame that
                    // changed many places that is many passes over one attachment differing in
                    // nothing but their scissor. Opening a render pass is not free — on the driver
                    // this was measured against it costs about 140 microseconds, so a frame with
                    // forty-eight of them spends most of its time opening them — and none of that
                    // buys anything here, because what separates the rectangles is the scissor and
                    // a scissor can be set inside one pass as often as one likes.
                    //
                    // So a run of passes that agree on everything else is opened once. A pass that
                    // *discards* its attachment breaks the run: that clear covers the whole
                    // attachment however the scissor is set, so folding it into the pass before it
                    // would throw away what that one had just drawn.
                    let mut end = index + 1;
                    while let Some(Segment::Pass(next)) = segments.get(end) {
                        if !shares_a_pass(first, next) {
                            break;
                        }
                        end += 1;
                    }
                    let run = segments[index..end]
                        .iter()
                        .filter_map(|segment| match segment {
                            Segment::Pass(pass) => Some(pass),
                            Segment::Encoder(_) | Segment::Round(_) => None,
                        });
                    self.record_pass(encoder, plan, run, &mut recorded);
                    index = end;
                }
                Segment::Round(_) => {}
            }
        }
        counter::add(Counter::DrawCalls, u64::from(recorded.draw_calls));
        if recorded.dropped > 0 {
            tracing::debug!(
                dropped = recorded.dropped,
                "planned draws the device could not issue"
            );
        }
        recorded
    }

    /// Runs one operation that needs the encoder itself.
    fn run(&self, encoder: &mut wgpu::CommandEncoder, op: EncoderOp) {
        match op {
            EncoderOp::Capture {
                source,
                destination,
                region,
            } => {
                let Some(from) = self.texture(source) else {
                    return;
                };
                let Some(to) = self.texture(destination) else {
                    return;
                };
                // Both targets are at one resolution, so the device region is the same number of
                // texels in each. Each side measures it from its own origin.
                debug_assert_eq!(source.scale(), destination.scale());
                let scale = source.scale();
                let read = scale.texels_at(region, source.origin());
                let write = scale.texels_at(region, destination.origin());
                let (read, write) = clamp_copy(read, write, size_of(from), size_of(to));
                if read.is_empty() {
                    return;
                }
                encoder.copy_texture_to_texture(
                    texel_copy(from, read),
                    texel_copy(to, write),
                    wgpu::Extent3d {
                        width: read.size.width as u32,
                        height: read.size.height as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
    }

    /// Opens one pass for a run of planned passes that share it, and issues all their draws.
    ///
    /// Every planned pass in `run` writes the same attachment under the same globals, so what
    /// distinguishes them is the scissor — set again before each one's draws.
    fn record_pass<'a>(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        plan: &FramePlan,
        run: impl Iterator<Item = &'a PlannedPass> + Clone,
        recorded: &mut Recorded,
    ) {
        let Some(planned) = run.clone().next() else {
            return;
        };
        // Nothing to draw into, which the attachment below would find for itself — asked here
        // because the run's own emptiness check comes first and would otherwise open nothing.
        if self.view(planned.target).is_none() {
            return;
        }
        let Some(format) = self.format(planned.target) else {
            return;
        };
        let extent = self.extent(planned.target);
        // Every scissor of the run is empty, so the whole run draws nothing anywhere.
        if run
            .clone()
            .all(|held| scaled(held.scissor, held.target, extent).is_empty())
        {
            return;
        }
        // Taken only for a pass that opens, because a query nothing writes would resolve to noise.
        let queries = self
            .timer
            .as_deref_mut()
            .and_then(|timer| timer.pass(timed_work(planned.target)));
        let Some(view) = self.view(planned.target) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("zgui.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: match planned.load {
                        PassLoad::Keep => wgpu::LoadOp::Load,
                        PassLoad::Discard => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: queries.as_ref().map(PassQueries::render),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // A target is allocated at a size *class*, so it is usually larger than the region it
        // holds. The viewport maps the target's texels, measured from its origin, onto the region
        // it covers; without it everything would be drawn at the wrong scale.
        pass.set_viewport(
            0.0,
            0.0,
            extent.width.max(1) as f32,
            extent.height.max(1) as f32,
            0.0,
            1.0,
        );
        let mut pass = BoundPass::new(pass);
        let tables = self
            .buffers
            .frame_bind_group(self.gpu, self.pipelines.layouts());
        for planned in run {
            let scissor = scaled(planned.scissor, planned.target, extent);
            if scissor.is_empty() {
                continue;
            }
            pass.set_scissor_rect(
                scissor.origin.x.max(0) as u32,
                scissor.origin.y.max(0) as u32,
                scissor.size.width.max(0) as u32,
                scissor.size.height.max(0) as u32,
            );
            for draw in plan.draws_of(planned) {
                let issued = self.issue(&mut pass, planned, tables.as_ref(), draw, format);
                if issued {
                    recorded.draw_calls += 1;
                } else {
                    recorded.dropped += 1;
                }
            }
        }
    }

    /// Issues one planned draw, and says whether it happened.
    fn issue(
        &mut self,
        pass: &mut BoundPass<'_>,
        planned: &PlannedPass,
        tables: Option<&wgpu::BindGroup>,
        draw: &PlannedDraw,
        format: wgpu::TextureFormat,
    ) -> bool {
        match draw {
            PlannedDraw::Clear => {
                let Some(pipeline) =
                    self.pipelines
                        .get(self.gpu, PipelineKind::DamageClear, format)
                else {
                    return false;
                };
                pass.set_pipeline(Bound::Kind(PipelineKind::DamageClear), pipeline);
                pass.draw(0..4, 0..1);
                true
            }
            PlannedDraw::Batch(batch) => self.batch(pass, planned, tables, batch.clone(), format),
            PlannedDraw::Blur {
                source,
                params,
                downsample,
            } => {
                let kind = if *downsample {
                    PipelineKind::BlurDownsample
                } else {
                    PipelineKind::BlurAxis
                };
                self.textured(pass, kind, *source, *params, None, format)
            }
            PlannedDraw::Effect {
                source,
                shader,
                params,
                block,
            } => self.effect_filter(pass, *source, *shader, *params, *block, format),
            PlannedDraw::Composite { source, params } => self.textured(
                pass,
                PipelineKind::Composite,
                *source,
                *params,
                tables.map(|bind| (bind, planned.globals)),
                format,
            ),
            PlannedDraw::Vector {
                target,
                first,
                count,
            } => {
                let Some(view) = self.vectors.and_then(|source| source.view(*target)) else {
                    return false;
                };
                let Some(bind) =
                    self.buffers
                        .vector_bind_group(self.gpu, self.pipelines.layouts(), view)
                else {
                    return false;
                };
                let Some(tables) = tables else {
                    return false;
                };
                let Some(pipeline) =
                    self.pipelines
                        .get(self.gpu, PipelineKind::VectorComposite, format)
                else {
                    return false;
                };
                pass.set_pipeline(Bound::Kind(PipelineKind::VectorComposite), pipeline);
                pass.set_bind_group(0, tables, &[planned.globals]);
                pass.set_bind_group(1, &bind, &[]);
                pass.draw(0..4, *first..*first + *count);
                true
            }
            PlannedDraw::Marks {
                kind,
                coverage,
                position,
                block,
                instances,
                texture,
            } => self.marks_draw(
                pass, planned, tables, *kind, *coverage, *position, *block, *instances, *texture,
                format,
            ),
            PlannedDraw::MarksComposite { block } => {
                let Some(view) = self.marks.array() else {
                    return false;
                };
                let Some(tables) = tables else {
                    return false;
                };
                let layouts = self.pipelines.layouts();
                let Some(instances) = self.buffers.instance_bind_group(
                    self.gpu,
                    layouts,
                    crate::renderer::frame::FrameBuffers::MARKS_LANE,
                ) else {
                    return false;
                };
                let Some(bins) = self.buffers.mark_composite_bind_group(
                    self.gpu,
                    layouts,
                    view,
                    self.marks.generation(),
                ) else {
                    return false;
                };
                let Some(pipeline) =
                    self.pipelines
                        .get(self.gpu, PipelineKind::MarksComposite, format)
                else {
                    return false;
                };
                pass.set_pipeline(Bound::Kind(PipelineKind::MarksComposite), pipeline);
                pass.set_bind_group(0, tables, &[planned.globals]);
                pass.set_bind_group(1, &instances, &[]);
                pass.set_bind_group(2, &bins, &[*block]);
                pass.draw(0..4, 0..1);
                true
            }
            PlannedDraw::External { texture, params } => {
                let Some(attached) = self.externals.get(texture) else {
                    return false;
                };
                let Some(bind) = self.buffers.filtered_bind_group(
                    self.gpu,
                    self.pipelines.layouts(),
                    &attached.view,
                    self.sampler,
                ) else {
                    return false;
                };
                let Some(tables) = tables else {
                    return false;
                };
                let Some(pipeline) = self.pipelines.get(self.gpu, PipelineKind::External, format)
                else {
                    return false;
                };
                pass.set_pipeline(Bound::Kind(PipelineKind::External), pipeline);
                pass.set_bind_group(0, tables, &[planned.globals]);
                pass.set_bind_group(1, &bind, &[*params]);
                pass.draw(0..4, 0..1);
                true
            }
        }
    }

    /// Issues one instanced batch of the display list.
    fn batch(
        &mut self,
        pass: &mut BoundPass<'_>,
        planned: &PlannedPass,
        tables: Option<&wgpu::BindGroup>,
        batch: Batch,
        format: wgpu::TextureFormat,
    ) -> bool {
        let (kind, range, texture) = match batch {
            Batch::Quads(range) => (PipelineKind::Quad, range, None),
            Batch::Shadows(range) => (PipelineKind::Shadow, range, None),
            Batch::Decorations(range) => (PipelineKind::Decoration, range, None),
            Batch::MonoSprites { texture, range } => {
                (PipelineKind::MonoSprite, range, Some(texture))
            }
            Batch::SubpixelSprites { texture, range } => {
                (PipelineKind::SubpixelSprite, range, Some(texture))
            }
            Batch::ColorSprites { texture, range } => {
                (PipelineKind::ColorSprite, range, Some(texture))
            }
            // An application effect binds a pipeline this crate never enumerated and a parameter
            // block of its own, so it is issued on its own path rather than through the table
            // above.
            Batch::Shaded {
                shader,
                params,
                range,
            } => return self.shaded(pass, planned, tables, shader, params, range, format),
            // Group markers, backdrops, vector composites and external quads are planned, not
            // batched: each one changes what is being drawn into or where the pixels come from.
            Batch::Group(_)
            | Batch::Backdrop(_)
            | Batch::Vector(_)
            | Batch::External(_)
            | Batch::Marks(_) => {
                return false;
            }
        };
        if range.is_empty() {
            return false;
        }
        let Some(tables) = tables else {
            return false;
        };
        let Some(lane) = crate::renderer::frame::FrameBuffers::lane(kind) else {
            return false;
        };
        let Some(instances) =
            self.buffers
                .instance_bind_group(self.gpu, self.pipelines.layouts(), lane)
        else {
            return false;
        };
        let atlas = match texture {
            None => None,
            // A sprite whose atlas texture was never created cannot be drawn at all: it happens
            // when a device was rebuilt and the content has not been rasterised again yet, and
            // drawing it against another texture would show a stranger's pixels.
            Some(texture) => match self.atlas.bind_group(decode_texture(texture)) {
                Some(bind_group) => Some(bind_group),
                None => return false,
            },
        };
        let Some(pipeline) = self.pipelines.get(self.gpu, kind, format) else {
            return false;
        };
        pass.set_pipeline(Bound::Kind(kind), pipeline);
        pass.set_bind_group(0, tables, &[planned.globals]);
        pass.set_bind_group(1, &instances, &[]);
        if let Some(bind_group) = atlas {
            pass.set_bind_group(2, bind_group, &[]);
        }
        pass.draw(0..4, range.start as u32..range.end as u32);
        true
    }

    /// Issues one payload kind of one mark item: one instance per prim, out of the payload arena.
    #[allow(clippy::too_many_arguments)]
    fn marks_draw(
        &mut self,
        pass: &mut BoundPass<'_>,
        planned: &PlannedPass,
        tables: Option<&wgpu::BindGroup>,
        kind: crate::pipeline::marks::MarkKind,
        coverage: bool,
        position: usize,
        block: u32,
        instances: [u32; 2],
        texture: Option<u32>,
        format: wgpu::TextureFormat,
    ) -> bool {
        let payload = self.buffers.chunks.mark_payload(position)[kind.lane()].clone();
        // The instances count from the payload start and never leave the payload. The whole
        // payload stays bound, so a segment at the start of the part still reads its neighbours.
        let start = payload.start.saturating_add(instances[0]).min(payload.end);
        let range = start..payload.start.saturating_add(instances[1]).min(payload.end);
        if range.is_empty() {
            return false;
        }
        let Some(tables) = tables else {
            return false;
        };
        let layouts = self.pipelines.layouts();
        let Some(instances) = self.buffers.instance_bind_group(
            self.gpu,
            layouts,
            crate::renderer::frame::FrameBuffers::MARKS_LANE,
        ) else {
            return false;
        };
        let Some(payload) = self.buffers.mark_bind_group(self.gpu, layouts, kind) else {
            return false;
        };
        // A glyph item whose atlas texture is gone draws nothing, as a sprite does: another
        // texture would show a stranger's pixels.
        let atlas = match texture {
            None => None,
            Some(texture) => match self.atlas.bind_group(decode_texture(texture)) {
                Some(bind_group) => Some(bind_group),
                None => return false,
            },
        };
        let pipeline_kind = PipelineKind::marks(kind, coverage);
        let Some(pipeline) = self.pipelines.get(self.gpu, pipeline_kind, format) else {
            return false;
        };
        pass.set_pipeline(Bound::Kind(pipeline_kind), pipeline);
        pass.set_bind_group(0, tables, &[planned.globals]);
        pass.set_bind_group(1, &instances, &[]);
        pass.set_bind_group(2, &payload, &[block]);
        if let Some(bind_group) = atlas {
            pass.set_bind_group(3, bind_group, &[]);
        }
        pass.draw(0..4, range);
        true
    }

    /// Issues one run of rectangles drawn by one application effect with one set of parameters.
    ///
    /// It binds what every instanced draw binds — the frame's tables and the lane's instances —
    /// and one block more. An effect the renderer was never told about draws nothing: the
    /// alternative is drawing the rectangle with whatever pipeline happened to be bound.
    #[allow(clippy::too_many_arguments)]
    fn shaded(
        &mut self,
        pass: &mut BoundPass<'_>,
        planned: &PlannedPass,
        tables: Option<&wgpu::BindGroup>,
        shader: zgui_scene::ShaderId,
        params: zgui_scene::ShaderParamsSlot,
        range: core::ops::Range<usize>,
        format: wgpu::TextureFormat,
    ) -> bool {
        if range.is_empty() {
            return false;
        }
        // Every one of these drops a rectangle the display list asked for, so each says once why:
        // an effect that stops appearing is otherwise indistinguishable from one drawing nothing.
        let Some(tables) = tables else {
            self.pipelines
                .note_undrawable_effect(shader, "the frame's side tables were not bound");
            return false;
        };
        let Some(offset) = self.buffers.effect_offset(params) else {
            self.pipelines
                .note_undrawable_effect(shader, "this frame staged no parameters for its block");
            return false;
        };
        let Some(block) = self
            .buffers
            .effect_bind_group(self.gpu, self.pipelines.layouts())
        else {
            self.pipelines
                .note_undrawable_effect(shader, "the parameter buffer was never uploaded");
            return false;
        };
        let lane = crate::renderer::frame::FrameBuffers::SHADED_LANE;
        let Some(instances) =
            self.buffers
                .instance_bind_group(self.gpu, self.pipelines.layouts(), lane)
        else {
            self.pipelines
                .note_undrawable_effect(shader, "the instance arena was not bound");
            return false;
        };
        let Some(pipeline) = self.pipelines.effect(self.gpu, shader, format) else {
            self.pipelines.note_undrawable_effect(
                shader,
                "no pipeline: the effect is not registered on this device, or would not build",
            );
            return false;
        };
        pass.set_pipeline(Bound::Effect(shader), pipeline);
        pass.set_bind_group(0, tables, &[planned.globals]);
        pass.set_bind_group(1, &instances, &[]);
        pass.set_bind_group(2, &block, &[offset]);
        pass.draw(0..4, range.start as u32..range.end as u32);
        true
    }

    /// Issues one filtering pass of an application's own shader.
    ///
    /// It binds what the blur chain binds — the block describing what it reads, the source and the
    /// sampler — and the effect's own parameters beside them. It binds none of the frame's tables:
    /// a filter is cut to its region by the scissor rather than clipped per fragment, so it reads
    /// no clip chain and there is nothing for it to index into.
    #[allow(clippy::too_many_arguments)]
    fn effect_filter(
        &mut self,
        pass: &mut BoundPass<'_>,
        source: TargetRef,
        shader: zgui_scene::ShaderId,
        params: u32,
        block: u32,
        format: wgpu::TextureFormat,
    ) -> bool {
        let Some(view) = self.view(source) else {
            return false;
        };
        let Some(read) = self.buffers.filtered_bind_group(
            self.gpu,
            self.pipelines.layouts(),
            view,
            self.sampler,
        ) else {
            return false;
        };
        let Some(own) = self
            .buffers
            .effect_bind_group(self.gpu, self.pipelines.layouts())
        else {
            return false;
        };
        let Some(pipeline) = self.pipelines.effect(self.gpu, shader, format) else {
            return false;
        };
        pass.set_pipeline(Bound::Effect(shader), pipeline);
        pass.set_bind_group(0, &read, &[params]);
        pass.set_bind_group(1, &own, &[block]);
        pass.draw(0..4, 0..1);
        true
    }

    /// Issues one draw that reads a target through a block of its own.
    fn textured(
        &mut self,
        pass: &mut BoundPass<'_>,
        kind: PipelineKind,
        source: TargetRef,
        params: u32,
        tables: Option<(&wgpu::BindGroup, u32)>,
        format: wgpu::TextureFormat,
    ) -> bool {
        let Some(view) = self.view(source) else {
            return false;
        };
        let Some(bind) = self.buffers.filtered_bind_group(
            self.gpu,
            self.pipelines.layouts(),
            view,
            self.sampler,
        ) else {
            return false;
        };
        let Some(pipeline) = self.pipelines.get(self.gpu, kind, format) else {
            return false;
        };
        pass.set_pipeline(Bound::Kind(kind), pipeline);
        let group = match tables {
            Some((bind_group, offset)) => {
                pass.set_bind_group(0, bind_group, &[offset]);
                1
            }
            None => 0,
        };
        pass.set_bind_group(group, &bind, &[params]);
        pass.draw(0..4, 0..1);
        true
    }

    /// A view of a target.
    fn view(&self, target: TargetRef) -> Option<&wgpu::TextureView> {
        match target {
            TargetRef::Composed => Some(self.composed.view()),
            TargetRef::Kept => Some(self.kept?.view()),
            TargetRef::Pool(slot) => Some(self.pool.view(slot)),
            TargetRef::MarksPage(page) => self.marks.page(page),
        }
    }

    /// A target's texture.
    fn texture(&self, target: TargetRef) -> Option<&wgpu::Texture> {
        match target {
            TargetRef::Composed => Some(self.composed.texture()),
            TargetRef::Kept => Some(self.kept?.texture()),
            TargetRef::Pool(slot) => Some(self.pool.texture(slot)),
            TargetRef::MarksPage(_) => None,
        }
    }

    /// A target's format.
    fn format(&self, target: TargetRef) -> Option<wgpu::TextureFormat> {
        match target {
            TargetRef::Composed => Some(self.composed.format()),
            TargetRef::Kept => Some(self.kept?.format()),
            TargetRef::Pool(slot) => Some(slot.format()),
            TargetRef::MarksPage(_) => Some(PipelineKind::COVERAGE_FORMAT),
        }
    }

    /// A target's extent in texels.
    fn extent(&self, target: TargetRef) -> Size<i32, Device> {
        match target {
            TargetRef::Composed | TargetRef::Kept => self.composed.used().size,
            TargetRef::Pool(slot) => slot.extent(),
            TargetRef::MarksPage(_) => self.mark_extent,
        }
    }
}

/// Which pipeline a pass has bound.
///
/// The attachment format is the same for every draw of one pass, so a kind or an effect names one
/// pipeline there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    /// One of this crate's own pipelines.
    Kind(PipelineKind),
    /// The pipeline of an application effect.
    Effect(zgui_scene::ShaderId),
}

/// The bind groups a pass can bind at once.
const BIND_SLOTS: usize = 4;

/// An open render pass, and what it has bound.
///
/// A pipeline, a bind group or a dynamic offset that is bound already is not issued again. Each
/// call the device records costs a validation and a merge of the bind group's resources into the
/// pass, and consecutive draws of one frame mostly bind the same tables and the same lane.
struct BoundPass<'pass> {
    /// The pass.
    pass: wgpu::RenderPass<'pass>,
    /// The pipeline bound last.
    pipeline: Option<Bound>,
    /// The bind group and the dynamic offset bound last at each index.
    groups: [Option<(wgpu::BindGroup, Option<u32>)>; BIND_SLOTS],
}

impl<'pass> BoundPass<'pass> {
    /// `pass`, with nothing bound yet.
    fn new(pass: wgpu::RenderPass<'pass>) -> Self {
        Self {
            pass,
            pipeline: None,
            groups: Default::default(),
        }
    }

    /// Binds `pipeline`, which `key` names, unless it is bound already.
    fn set_pipeline(&mut self, key: Bound, pipeline: &wgpu::RenderPipeline) {
        if self.pipeline != Some(key) {
            self.pass.set_pipeline(pipeline);
            self.pipeline = Some(key);
        }
    }

    /// Sets the scissor the draws that follow are cut to.
    ///
    /// Not cached: it is the one thing that differs between the planned passes of a run, so a set
    /// of it is the reason the run could be folded into one pass at all.
    fn set_scissor_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.pass.set_scissor_rect(x, y, width, height);
    }

    /// Binds `group` at `index` with `offsets`, unless the same group and offset are bound there.
    fn set_bind_group(&mut self, index: u32, group: &wgpu::BindGroup, offsets: &[u32]) {
        debug_assert!(
            offsets.len() <= 1,
            "every bind group here has one dynamic offset at most"
        );
        let offset = offsets.first().copied();
        let held = &mut self.groups[index as usize];
        if held
            .as_ref()
            .is_some_and(|(bound, at)| bound == group && *at == offset)
        {
            return;
        }
        self.pass.set_bind_group(index, group, offsets);
        *held = Some((group.clone(), offset));
    }

    /// Draws `instances` of a strip of `vertices`.
    fn draw(&mut self, vertices: core::ops::Range<u32>, instances: core::ops::Range<u32>) {
        self.pass.draw(vertices, instances);
    }
}

/// The kind of work a pass into `target` is timed as.
fn timed_work(target: TargetRef) -> TimedWork {
    match target {
        // A pass into the kept copy is a filter's own work, as a pass into a lease is, and not
        // the document's. One only opens there to clear what a partial copy does not write.
        TargetRef::Pool(_) | TargetRef::Kept => TimedWork::Groups,
        TargetRef::Composed | TargetRef::MarksPage(_) => TimedWork::Draws,
    }
}

/// A device-pixel rectangle in a target's own texels, cut to its extent.
fn scaled(
    rect: Rect<i32, Device>,
    target: TargetRef,
    extent: Size<i32, Device>,
) -> Rect<i32, Device> {
    target
        .scale()
        .texels_at(rect, target.origin())
        .intersection(Rect::new(zgui_geom::Point::new(0, 0), extent))
        .unwrap_or(Rect::ZERO)
}

/// The two sides of a copy, cut to what a source of size `from` and a destination of size `to`
/// actually hold.
///
/// `read` and `write` are the same size, at different texels when the two targets start at
/// different device pixels. Each cut applies to both sides, so they stay the same size.
fn clamp_copy(
    read: Rect<i32, Device>,
    write: Rect<i32, Device>,
    from: Size<i32, Device>,
    to: Size<i32, Device>,
) -> (Rect<i32, Device>, Rect<i32, Device>) {
    let bound = |size| Rect::new(zgui_geom::Point::new(0, 0), size);
    // The destination's bounds in the source's texels, and back.
    let forward = read.origin.offset_from(write.origin);
    let back = write.origin.offset_from(read.origin);
    match read
        .intersection(bound(from))
        .and_then(|clipped| clipped.intersection(bound(to).translate(forward)))
    {
        Some(read) => (read, read.translate(back)),
        None => (Rect::ZERO, Rect::ZERO),
    }
}

/// The texel size of `texture`.
fn size_of(texture: &wgpu::Texture) -> Size<i32, Device> {
    Size::new(texture.width() as i32, texture.height() as i32)
}

/// One corner of a copy between two textures.
fn texel_copy(
    texture: &wgpu::Texture,
    region: Rect<i32, Device>,
) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d {
            x: region.origin.x.max(0) as u32,
            y: region.origin.y.max(0) as u32,
            z: 0,
        },
        aspect: wgpu::TextureAspect::All,
    }
}

/// The atlas texture a batch's packed identifier names.
fn decode_texture(packed: u32) -> zgui_atlas::TextureId {
    let kind = match packed >> 16 {
        0 => zgui_atlas::TextureKind::Mono,
        1 => zgui_atlas::TextureKind::Subpixel,
        2 => zgui_atlas::TextureKind::Color,
        _ => zgui_atlas::TextureKind::Image,
    };
    zgui_atlas::TextureId::new(kind, packed & 0xffff)
}

/// Whether `next` can be issued inside the pass `first` opens.
///
/// It can when the two write the same attachment under the same globals and `next` keeps what is
/// there. A pass that discards cannot join one: its clear covers the whole attachment whatever the
/// scissor says, so it would throw away everything drawn before it in that pass.
fn shares_a_pass(first: &PlannedPass, next: &PlannedPass) -> bool {
    first.target == next.target
        && first.globals == next.globals
        && matches!(next.load, PassLoad::Keep)
}

#[cfg(test)]
mod tests {
    use super::{clamp_copy, decode_texture, shares_a_pass};
    use crate::frame::segment::{PassLoad, PlannedPass};
    use crate::frame::target::TargetRef;
    use zgui_atlas::{AtlasTile, TextureId, TextureKind, TileId};
    use zgui_geom::{Point, Rect, Size};
    use zgui_scene::SpriteTile;

    #[test]
    fn a_packed_texture_identifier_round_trips_through_a_batch() {
        // The display list packs the pool and the index into one number so that a batch can break
        // on a change of texture; this is the other end of that packing, and the two agreeing is
        // what stops a colour sprite being drawn against the coverage pool.
        for kind in TextureKind::ALL {
            for index in [0, 1, 7] {
                let tile = AtlasTile {
                    texture: TextureId::new(kind, index),
                    tile: TileId(0),
                    bounds: Rect::new(Point::new(0, 0), Size::new(1, 1)),
                };
                let packed = SpriteTile::of(tile).texture;
                assert_eq!(decode_texture(packed), TextureId::new(kind, index));
            }
        }
    }

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, zgui_geom::Device> {
        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    #[test]
    fn a_smaller_destination_at_another_origin_cuts_both_sides_of_a_copy() {
        // Device pixels (10,10)-(70,70), from a 100x100 source at the device origin into a 40x40
        // destination that starts at device (20,20).
        let (read, write) = clamp_copy(
            rect(10, 10, 60, 60),
            rect(-10, -10, 60, 60),
            Size::new(100, 100),
            Size::new(40, 40),
        );
        assert_eq!(write, rect(0, 0, 40, 40));
        assert_eq!(read, rect(20, 20, 40, 40));

        // A 50x50 source cuts the far edge first, and the destination cuts the near edge.
        let (read, write) = clamp_copy(
            rect(10, 10, 60, 60),
            rect(-10, -10, 60, 60),
            Size::new(50, 50),
            Size::new(40, 40),
        );
        assert_eq!(read, rect(20, 20, 30, 30));
        assert_eq!(write, rect(0, 0, 30, 30));
    }

    #[test]
    fn a_copy_outside_the_destination_copies_nothing() {
        let (read, write) = clamp_copy(
            rect(0, 0, 10, 10),
            rect(50, 50, 10, 10),
            Size::new(100, 100),
            Size::new(40, 40),
        );
        assert_eq!((read, write), (Rect::ZERO, Rect::ZERO));
    }

    /// A planned pass over the composed target, as a damage rectangle is planned.
    fn planned(scissor: Rect<i32, zgui_geom::Device>, load: PassLoad, globals: u32) -> PlannedPass {
        PlannedPass {
            target: TargetRef::Composed,
            load,
            scissor,
            globals,
            draws: 0..1,
        }
    }

    #[test]
    fn the_passes_of_scattered_damage_rectangles_are_one_run() {
        // The case the coalescing exists for: a frame that changed several places plans a pass per
        // place, and they differ in nothing but their scissor. Opening one costs about 140
        // microseconds, and a scissor can be set inside one pass as often as one likes.
        let first = planned(
            Rect::new(Point::new(0, 0), Size::new(16, 16)),
            PassLoad::Keep,
            0,
        );
        for away in [Point::new(64, 0), Point::new(0, 64), Point::new(900, 700)] {
            let next = planned(Rect::new(away, Size::new(16, 16)), PassLoad::Keep, 0);
            assert!(
                shares_a_pass(&first, &next),
                "a rectangle at {away:?} differs from the first only in its scissor"
            );
        }
    }

    #[test]
    fn a_pass_that_discards_its_attachment_breaks_the_run() {
        // The one rule that keeps this exact. A discard clears the whole attachment however the
        // scissor is set, so folding it into the pass before it would throw away what that pass
        // had just drawn.
        let first = planned(
            Rect::new(Point::new(0, 0), Size::new(16, 16)),
            PassLoad::Keep,
            0,
        );
        let discarding = planned(
            Rect::new(Point::new(64, 0), Size::new(16, 16)),
            PassLoad::Discard,
            0,
        );
        assert!(!shares_a_pass(&first, &discarding));
    }

    #[test]
    fn a_run_does_not_cross_an_attachment_or_a_change_of_globals() {
        // Both are properties of the pass rather than of the rectangle, and neither can be set
        // again inside one: the attachment is what the pass was opened on, and the globals are
        // bound with it.
        let first = planned(
            Rect::new(Point::new(0, 0), Size::new(16, 16)),
            PassLoad::Keep,
            0,
        );
        let elsewhere = PlannedPass {
            target: TargetRef::MarksPage(0),
            ..planned(
                Rect::new(Point::new(64, 0), Size::new(16, 16)),
                PassLoad::Keep,
                0,
            )
        };
        assert!(!shares_a_pass(&first, &elsewhere), "another attachment");
        let rescaled = planned(
            Rect::new(Point::new(64, 0), Size::new(16, 16)),
            PassLoad::Keep,
            1,
        );
        assert!(!shares_a_pass(&first, &rescaled), "other globals");
    }
}
