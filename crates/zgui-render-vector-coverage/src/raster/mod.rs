//! The rasteriser: flatten on the host, multisample on the device, one resolve per scratch layer.

pub mod geometry;
pub mod instance;
pub mod pipeline;
pub mod scratch;

use std::sync::Arc;

use kurbo::Affine;
use zgui_geom::{Device, Matrix4, Rect};
use zgui_render::{
    Layering, MemoryReport, VectorError, VectorFrame, VectorPass, VectorPlan, VectorRaster,
    VectorTarget,
};
use zgui_render_wgpu::Gpu;
use zgui_render_wgpu::frame::vector::VectorSource;
use zgui_scene::{PaintTable, ScenePassPlan, VectorItem};

use crate::raster::geometry::Segment;
use crate::raster::instance::{Item, Run};
use crate::raster::pipeline::{Pipelines, Storage};
use crate::raster::scratch::Scratch;

/// What one frame's rasterisation cost and could not do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rasterised {
    /// Residual clip outlines applied inside the scratch.
    pub clip_layers: u32,
    /// Items left undrawn because a residual clip had no shape to apply.
    pub unclippable: u32,
    /// Items left undrawn because nothing here paints what they asked for.
    pub unpaintable: u32,
    /// Items whose transform is not two-dimensional, drawn without it.
    pub flattened_transforms: u32,
    /// Line segments the frame flattened.
    pub segments: u32,
}

/// A vector rasteriser that needs no compute shaders.
///
/// See the crate documentation for what switching to this costs; it is a visible downgrade and not
/// a transparent one.
#[derive(Debug)]
pub struct CoverageRaster {
    /// The device it draws on.
    gpu: Arc<Gpu>,
    /// The pipelines.
    pipelines: Pipelines,
    /// The two textures a pass passes through.
    scratch: Scratch,
    /// Every outline of the frame, end to end.
    segments: Vec<Segment>,
    /// Where each residual clip's outline is.
    runs: Vec<Run>,
    /// What to fill, in the order it is filled.
    items: Vec<Item>,
    /// Which instances belong to which pass.
    spans: Vec<(u32, u32)>,
    /// The regions the layering is computed from, kept so that a frame allocates nothing for it.
    regions: Vec<Rect<i32, Device>>,
    /// Which passes went into which layer, kept for the same reason.
    layered: Vec<Vec<usize>>,
    /// The buffers the three of those are uploaded to.
    buffers: Buffers,
    /// What the last frame cost and could not do.
    last: Rasterised,
    /// How many layers the last frame's passes needed.
    depth: u32,
}

/// The device-side copies of the three host arrays.
#[derive(Debug)]
struct Buffers {
    /// The items.
    items: Storage,
    /// The segments.
    segments: Storage,
    /// The clip runs.
    runs: Storage,
}

impl CoverageRaster {
    /// A rasteriser on `gpu`, sized for a surface of `width` by `height` device pixels.
    pub fn new(gpu: &Arc<Gpu>, width: u32, height: u32) -> Self {
        let mut scratch = Scratch::new();
        scratch.ensure(gpu, width.max(1), height.max(1), Scratch::LAYERS);
        Self {
            pipelines: Pipelines::new(gpu),
            scratch,
            segments: Vec::new(),
            runs: Vec::new(),
            items: Vec::new(),
            spans: Vec::new(),
            regions: Vec::new(),
            layered: Vec::new(),
            buffers: Buffers {
                items: Storage::new(gpu, "zgui.vector.coverage.items"),
                segments: Storage::new(gpu, "zgui.vector.coverage.segments"),
                runs: Storage::new(gpu, "zgui.vector.coverage.runs"),
            },
            gpu: Arc::clone(gpu),
            last: Rasterised::default(),
            depth: 0,
        }
    }

    /// What the last frame cost, and what it could not do.
    pub fn last_frame(&self) -> Rasterised {
        self.last
    }

    /// How many scratch layers the last frame's passes needed.
    ///
    /// The frame's own demand, which is what says how much of it overlapped — not how many layers
    /// are allocated, which never falls below a floor and follows the demand down only slowly.
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// How many scratch layers are allocated, in each of the two textures.
    pub fn layers(&self) -> u32 {
        self.scratch.layers()
    }

    /// The extent every scratch layer is allocated at.
    pub fn extent(&self) -> (u32, u32) {
        self.scratch.extent()
    }

    /// The bytes of video memory both scratch textures occupy.
    pub fn scratch_bytes(&self) -> u64 {
        self.scratch.bytes()
    }

    /// Turns one pass's items into fills, and says how many there are.
    fn collect(&mut self, frame: &VectorFrame<'_>, pass: &VectorPass) -> (u32, u32) {
        let first = self.items.len() as u32;
        // The *layer's* extent, because the attachment a draw is mapped onto is the whole layer.
        let (layer_width, layer_height) = self.scratch.extent();
        let extent = [layer_width as f32, layer_height as f32, 0.0, 0.0];
        // Nothing at all: a layer is in the surface's own coordinates, which is what lets two passes
        // that do not meet on the screen share it. An outline is already in device space and stays
        // there.
        let shift = Affine::translate((
            f64::from(pass.raster_region.origin.x - pass.region.origin.x),
            f64::from(pass.raster_region.origin.y - pass.region.origin.y),
        ));
        let origin = pass.raster_region.origin;

        for planned in frame.plan.items_of(pass) {
            let Some(item) = frame.items.get(planned.item) else {
                continue;
            };
            let links = frame.clips.links(planned.residual);
            let Some(shapes) = links
                .iter()
                .map(zgui_scene::clip::path::of)
                .collect::<Option<Vec<_>>>()
            else {
                self.last.unclippable += 1;
                continue;
            };
            // The residual is a clip chain, so it is already in device space and the layer is too:
            // it is placed unchanged, never through the item's own transform.
            let clip_first = self.runs.len();
            for shape in &shapes {
                let start = self.segments.len();
                geometry::flatten(shape, shift, &mut self.segments);
                self.runs.push(Run::new(start, self.segments.len() - start));
                self.last.clip_layers += 1;
            }

            let placement = shift * self.transform_of(item, frame);
            // The item's own shape clips are in the item's own space, so unlike the residual they
            // go through the item's transform: a clipped drawing that is rotated has its clip
            // rotated with it.
            for clip in &item.clips {
                let start = self.segments.len();
                geometry::flatten(&clip.path, placement, &mut self.segments);
                self.runs.push(Run::of(
                    start,
                    self.segments.len() - start,
                    clip.rule == peniko::Fill::EvenOdd,
                ));
                self.last.clip_layers += 1;
            }
            let clip_count = self.runs.len() - clip_first;

            // The ink is recorded relative to its pass's region and the layer is in device
            // coordinates, so the two are added back together here.
            let bounds = [
                (origin.x + planned.ink.origin.x) as f32 - 1.0,
                (origin.y + planned.ink.origin.y) as f32 - 1.0,
                planned.ink.size.width as f32 + 2.0,
                planned.ink.size.height as f32 + 2.0,
            ];
            let mut painted = false;
            if let Some(color) = flat(item.fill, frame.paints) {
                let start = self.segments.len();
                geometry::flatten(&item.path, placement, &mut self.segments);
                self.items.push(Item {
                    bounds,
                    viewport: extent,
                    color,
                    control: [
                        start as f32,
                        (self.segments.len() - start) as f32,
                        f32::from(u8::from(item.fill_rule == peniko::Fill::EvenOdd)),
                        clip_first as f32,
                    ],
                    clips: [clip_count as f32, 0.0, 0.0, 0.0],
                });
                painted = true;
            }
            if let Some(stroke) = item.stroke.as_ref()
                && let Some(color) = flat(Some(stroke.paint), frame.paints)
            {
                let start = self.segments.len();
                geometry::flatten_stroke(&item.path, &stroke.style, placement, &mut self.segments);
                self.items.push(Item {
                    bounds,
                    viewport: extent,
                    color,
                    // A stroke's outline is always filled by the non-zero rule whatever the fill
                    // rule of the shape it came from: the outline is a boundary, not a region the
                    // author wrote a rule for.
                    control: [
                        start as f32,
                        (self.segments.len() - start) as f32,
                        0.0,
                        clip_first as f32,
                    ],
                    clips: [clip_count as f32, 0.0, 0.0, 0.0],
                });
                painted = true;
            }
            if !painted {
                self.last.unpaintable += 1;
            }
        }
        (first, self.items.len() as u32 - first)
    }

    /// The item's own transform, counting the ones this cannot apply.
    fn transform_of(&mut self, item: &VectorItem, frame: &VectorFrame<'_>) -> Affine {
        let Some(id) = item.transform else {
            return Affine::IDENTITY;
        };
        let Some(matrix) = frame.placements.get(id) else {
            return Affine::IDENTITY;
        };
        if !matrix.is_2d() {
            self.last.flattened_transforms += 1;
            return Affine::IDENTITY;
        }
        affine_of(matrix)
    }

    /// Sorts the passes that were given a layer into the layer each one went into.
    fn group(&mut self, passes: &[VectorPass]) {
        let layers = (self.scratch.layers() as usize).max(1);
        for bucket in &mut self.layered {
            bucket.clear();
        }
        self.layered.resize_with(layers, Vec::new);
        for (index, pass) in passes.iter().enumerate() {
            if let Some(bucket) = self.layered.get_mut(pass.target.0 as usize) {
                bucket.push(index);
            }
        }
    }

    /// Records every pass of the frame into one encoder and submits it.
    ///
    /// One accumulation pass and one resolve per *layer*, not per pass: the passes sharing a layer
    /// are disjoint on the surface, so their outlines accumulate side by side and one draw converts
    /// the whole layer into what a composite reads. Resolving per pass would convert the same layer
    /// once per pass in it, and every conversion after the first would read what the one before it
    /// had already written.
    fn record(&self) -> Result<(), VectorError> {
        let bind = self
            .gpu
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("zgui.vector.coverage"),
                layout: &self.pipelines.coverage_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.items.binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.buffers.segments.binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.buffers.runs.binding(),
                    },
                ],
            });
        let mut encoder =
            self.gpu
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("zgui.vector.coverage"),
                });
        for (layer, indices) in self.layered.iter().enumerate() {
            if indices.is_empty() {
                continue;
            }
            let layer = layer as u32;
            let (Some(accumulation), Some(straight)) = (
                self.scratch.accumulation(layer),
                self.scratch.straight(layer),
            ) else {
                return Err(VectorError::Allocation {
                    detail: format!("no scratch layer {layer} was allocated"),
                });
            };
            {
                let mut render = begin(&mut encoder, accumulation, "zgui.vector.coverage");
                render.set_pipeline(&self.pipelines.coverage);
                render.set_bind_group(0, &bind, &[]);
                for &index in indices {
                    let (first, count) = self.spans[index];
                    if count > 0 {
                        render.draw(0..4, first..first + count);
                    }
                }
            }
            let resolve_bind = self
                .gpu
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("zgui.vector.coverage.resolve"),
                    layout: &self.pipelines.resolve_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(accumulation),
                    }],
                });
            let mut render = begin(&mut encoder, straight, "zgui.vector.coverage.resolve");
            render.set_pipeline(&self.pipelines.resolve);
            render.set_bind_group(0, &resolve_bind, &[]);
            render.draw(0..4, 0..1);
        }
        self.gpu.queue().submit([encoder.finish()]);
        Ok(())
    }
}

impl VectorRaster for CoverageRaster {
    fn plan(&mut self, passes: &ScenePassPlan) -> VectorPlan {
        if passes.is_empty() {
            return VectorPlan::empty();
        }
        let mut plan = VectorPlan::resourcing(passes);
        // Layers are shared by passes that do not meet on the surface: every pass is rasterised
        // before any of them is composited, so a layer holds its passes' coverage until their
        // composites have read it, and passes that do not overlap in device coordinates do not
        // overlap in a layer that is in device coordinates either.
        self.regions.clear();
        self.regions
            .extend(passes.passes.iter().map(|planned| planned.region));
        let layering = Layering::of(&self.regions, Scratch::MAX_LAYERS);
        let (packed, width, height) = layering.compact(&self.regions);
        self.depth = layering.layers();
        for (index, planned) in passes.passes.iter().enumerate() {
            plan.passes.push(VectorPass {
                region: planned.region,
                raster_region: packed[index],
                target: layering.target(index),
                items: planned.items.clone(),
                clip: planned.clip,
                instanced: planned.instanced,
            });
        }
        // The far corner of the surface anything is drawn at, not the largest region: a layer holds
        // device pixels where they belong, so it has to reach as far as the furthest of them.
        self.scratch
            .ensure(&self.gpu, width, height, layering.layers());
        plan
    }

    fn clear_targets(&mut self, plan: &VectorPlan) {
        let mut layers: Vec<u32> = plan
            .passes
            .iter()
            .filter(|pass| pass.target != VectorTarget::NONE)
            .map(|pass| pass.target.0 as u32)
            .collect();
        layers.sort_unstable();
        layers.dedup();
        self.scratch.clear(&self.gpu, &layers);
    }

    fn prepare(&mut self, frame: &mut VectorFrame<'_>) -> Result<(), VectorError> {
        self.last = Rasterised::default();
        self.segments.clear();
        self.runs.clear();
        self.items.clear();
        self.spans.clear();
        if frame.is_empty() {
            return Ok(());
        }
        // The passes that were given a layer are a prefix, so the ones that were not are exactly the
        // tail a shortened plan drops — and a composite is named by its index, so it has to be a
        // tail and not a scattering.
        let prepared = frame
            .plan
            .passes
            .iter()
            .position(|pass| pass.target == VectorTarget::NONE)
            .unwrap_or(frame.plan.passes.len());
        for index in 0..prepared {
            let pass = frame.plan.passes[index].clone();
            let span = self.collect(frame, &pass);
            self.spans.push(span);
        }
        self.group(&frame.plan.passes[..prepared]);
        self.last.segments = self.segments.len() as u32;
        self.buffers.items.write(&self.gpu, &self.items);
        self.buffers.segments.write(&self.gpu, &self.segments);
        self.buffers.runs.write(&self.gpu, &self.runs);
        self.record()?;
        if self.last.unclippable > 0 {
            tracing::warn!(
                items = self.last.unclippable,
                "vector items left undrawn because a residual clip had no shape to apply"
            );
        }
        if prepared < frame.plan.passes.len() {
            // More passes stacked over one point than there are layers to keep them apart. Reporting
            // it is what makes this frame's vector content missing rather than jumbled: the
            // alternative is two overlapping passes on one layer, and then one composite draws the
            // other's outlines.
            return Err(VectorError::OutOfCapacity {
                detail: format!(
                    "a frame planned {} passes and {} of them could be given one of {} layers",
                    frame.plan.passes.len(),
                    prepared,
                    Scratch::MAX_LAYERS
                ),
                prepared,
            });
        }
        Ok(())
    }

    fn memory(&self) -> MemoryReport {
        MemoryReport {
            // Nothing fixed at all, which is the whole shape of this rasteriser against the other
            // one: it holds two scratch textures and three buffers, and not one byte that does not
            // scale with what is drawn.
            fixed: 0,
            scratch: self.scratch.bytes(),
            buffers: self.buffers.items.capacity()
                + self.buffers.segments.capacity()
                + self.buffers.runs.capacity(),
            ..MemoryReport::ZERO
        }
    }

    fn release_idle_resources(&mut self) -> u64 {
        let mut freed = self.scratch.release();
        freed += self.buffers.items.shrink(&self.gpu);
        freed += self.buffers.segments.shrink(&self.gpu);
        freed += self.buffers.runs.shrink(&self.gpu);
        self.items.clear();
        self.items.shrink_to_fit();
        self.segments.clear();
        self.segments.shrink_to_fit();
        self.runs.clear();
        self.runs.shrink_to_fit();
        freed
    }
}

impl VectorSource for CoverageRaster {
    fn view(&self, target: VectorTarget) -> Option<&wgpu::TextureView> {
        self.scratch.straight(target.0 as u32)
    }
}

/// Opens a render pass that keeps what the attachment already holds.
fn begin<'encoder>(
    encoder: &'encoder mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::RenderPass<'encoder> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                // What is already there is this frame's pre-clear, and outlines composite over each
                // other, so it is kept rather than cleared again.
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// The straight, gamma-encoded colour a paint reference is drawn in, or `None` for one this
/// cannot draw at all.
fn flat(reference: Option<zgui_scene::PaintRef>, paints: &PaintTable) -> Option<[f32; 4]> {
    let entry = paints.get(reference?.id()?)?;
    // A ramp needs a per-fragment evaluation this deliberately does not have, so it is filled with
    // its mean colour: the shape still appears, which is the difference between a gradient-filled
    // icon looking flat on a device with no compute shaders and it not being there at all. A
    // sampled image has no such stand-in and is still not drawn.
    let color = entry.flat_color()?;
    let srgb = color.to_space(zgui_color::ColorSpace::Srgb);
    let [red, green, blue] = srgb.components();
    Some([red, green, blue, srgb.alpha()])
}

/// The two-dimensional affine a matrix embeds.
fn affine_of(matrix: &Matrix4) -> Affine {
    let column = matrix.columns;
    Affine::new([
        f64::from(column[0][0]),
        f64::from(column[0][1]),
        f64::from(column[1][0]),
        f64::from(column[1][1]),
        f64::from(column[3][0]),
        f64::from(column[3][1]),
    ])
}
