//! Recording a planned frame into one command encoder.

use std::collections::BTreeMap;

use zgui_geom::{Device, Rect};
use zgui_profile::{Counter, counter};
use zgui_scene::{Batch, ExternalTextureId};

use crate::atlas_backend::sink::AtlasTextures;
use crate::frame::build::FramePlan;
use crate::frame::segment::{EncoderOp, PassLoad, PlannedDraw, PlannedPass};
use crate::frame::target::TargetRef;
use crate::gpu::device::Gpu;
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
    /// The sampler every magnifying read goes through.
    pub sampler: &'frame wgpu::Sampler,
    /// Textures the renderer did not draw.
    pub externals: &'frame BTreeMap<ExternalTextureId, AttachedTexture>,
    /// Whatever rasterised this frame's vector content, when there is one.
    pub vectors: Option<&'frame dyn crate::frame::vector::VectorSource>,
}

impl Recorder<'_> {
    /// Records `plan` into `encoder`.
    ///
    /// One pass is opened per pass segment and dropped before the next segment, because a live
    /// pass holds the encoder borrowed and the operations between passes need it. That constraint
    /// is not worked around here — it is what the plan is a plan *of*.
    pub fn record(&mut self, encoder: &mut wgpu::CommandEncoder, plan: &FramePlan) -> Recorded {
        let mut recorded = Recorded::default();
        for segment in &plan.segments {
            match segment {
                crate::frame::segment::Segment::Encoder(op) => self.run(encoder, *op),
                crate::frame::segment::Segment::Pass(pass) => {
                    self.record_pass(encoder, plan, pass, &mut recorded);
                }
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
                let region = clamp(region, from, to);
                if region.is_empty() {
                    return;
                }
                encoder.copy_texture_to_texture(
                    texel_copy(from, region),
                    texel_copy(to, region),
                    wgpu::Extent3d {
                        width: region.size.width as u32,
                        height: region.size.height as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
    }

    /// Opens one pass and issues its draws.
    fn record_pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        plan: &FramePlan,
        planned: &PlannedPass,
        recorded: &mut Recorded,
    ) {
        let Some(view) = self.view(planned.target) else {
            return;
        };
        let Some(format) = self.format(planned.target) else {
            return;
        };
        let extent = self.extent(planned.target);
        let scissor = scaled(planned.scissor, planned.target, extent);
        if scissor.is_empty() {
            return;
        }
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
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // A target is allocated at a size *class*, so it is usually larger than the region it
        // holds. The viewport maps device coordinates onto the region rather than onto the whole
        // allocation; without it everything would be drawn at the wrong scale.
        pass.set_viewport(
            0.0,
            0.0,
            extent.width.max(1) as f32,
            extent.height.max(1) as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(
            scissor.origin.x.max(0) as u32,
            scissor.origin.y.max(0) as u32,
            scissor.size.width.max(0) as u32,
            scissor.size.height.max(0) as u32,
        );
        let tables = self
            .buffers
            .frame_bind_group(self.gpu, self.pipelines.layouts());
        for draw in plan.draws_of(planned) {
            let issued = self.issue(&mut pass, planned, tables.as_ref(), draw, format);
            if issued {
                recorded.draw_calls += 1;
            } else {
                recorded.dropped += 1;
            }
        }
    }

    /// Issues one planned draw, and says whether it happened.
    fn issue(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
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
                pass.set_pipeline(pipeline);
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
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, tables, &[planned.globals]);
                pass.set_bind_group(1, &bind, &[]);
                pass.draw(0..4, *first..*first + *count);
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
                pass.set_pipeline(pipeline);
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
        pass: &mut wgpu::RenderPass<'_>,
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
            Batch::Group(_) | Batch::Backdrop(_) | Batch::Vector(_) | Batch::External(_) => {
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, tables, &[planned.globals]);
        pass.set_bind_group(1, &instances, &[]);
        if let Some(bind_group) = atlas {
            pass.set_bind_group(2, bind_group, &[]);
        }
        pass.draw(0..4, range.start as u32..range.end as u32);
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
        pass: &mut wgpu::RenderPass<'_>,
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
        pass.set_pipeline(pipeline);
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
        pass: &mut wgpu::RenderPass<'_>,
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &read, &[params]);
        pass.set_bind_group(1, &own, &[block]);
        pass.draw(0..4, 0..1);
        true
    }

    /// Issues one draw that reads a target through a block of its own.
    fn textured(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
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
        pass.set_pipeline(pipeline);
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
            TargetRef::Pool(slot) => Some(self.pool.view(slot)),
        }
    }

    /// A target's texture.
    fn texture(&self, target: TargetRef) -> Option<&wgpu::Texture> {
        match target {
            TargetRef::Composed => Some(self.composed.texture()),
            TargetRef::Pool(slot) => Some(self.pool.texture(slot)),
        }
    }

    /// A target's format.
    fn format(&self, target: TargetRef) -> Option<wgpu::TextureFormat> {
        match target {
            TargetRef::Composed => Some(self.composed.format()),
            TargetRef::Pool(slot) => Some(slot.format()),
        }
    }

    /// A target's extent in texels.
    fn extent(&self, target: TargetRef) -> zgui_geom::Size<i32, Device> {
        match target {
            TargetRef::Composed => self.composed.used().size,
            TargetRef::Pool(slot) => slot.scale().extent(self.pool.region()),
        }
    }
}

/// A device-pixel rectangle in a target's own texels, cut to its extent.
fn scaled(
    rect: Rect<i32, Device>,
    target: TargetRef,
    extent: zgui_geom::Size<i32, Device>,
) -> Rect<i32, Device> {
    let scale = target.scale();
    let scaled = Rect::from_corners(
        zgui_geom::Point::new(scale.texel(rect.left()), scale.texel(rect.top())),
        zgui_geom::Point::new(
            scale.texel(rect.right().max(rect.left())),
            scale.texel(rect.bottom().max(rect.top())),
        ),
    );
    scaled
        .intersection(Rect::new(zgui_geom::Point::new(0, 0), extent))
        .unwrap_or(Rect::ZERO)
}

/// `region` cut to what both textures actually hold.
fn clamp(region: Rect<i32, Device>, from: &wgpu::Texture, to: &wgpu::Texture) -> Rect<i32, Device> {
    let bound = |texture: &wgpu::Texture| {
        Rect::new(
            zgui_geom::Point::new(0, 0),
            zgui_geom::Size::new(texture.width() as i32, texture.height() as i32),
        )
    };
    region
        .intersection(bound(from))
        .and_then(|clipped| clipped.intersection(bound(to)))
        .unwrap_or(Rect::ZERO)
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

#[cfg(test)]
mod tests {
    use super::decode_texture;
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
}
