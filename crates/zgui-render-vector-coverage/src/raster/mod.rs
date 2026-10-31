//! The rasteriser: flatten on the host, multisample on the device, one resolve per scratch layer.

pub mod geometry;
pub mod instance;
pub mod pipeline;
pub mod scratch;

use std::sync::Arc;

use kurbo::Affine;
use zgui_geom::{Device, Matrix4, Rect};
use zgui_render::{
    Capacity, Layering, MemoryReport, VectorError, VectorFrame, VectorPass, VectorPlan,
    VectorRaster, VectorTarget,
};
use zgui_render_wgpu::Gpu;
use zgui_render_wgpu::frame::vector::VectorSource;
use zgui_scene::{PaintTable, ScenePassPlan, VectorItem};

use crate::raster::geometry::Segment;

/// Which of one item's two outlines a cache entry is for.
///
/// A path can be both filled and stroked, and the two flatten to different outlines — a stroke is
/// expanded into the boundary of the stroked region before it is flattened at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct OutlineKey {
    /// The item, by the identity the scene keeps stable across frames for exactly this.
    id: zgui_scene::VectorId,
    /// Whether this is the stroke's outline rather than the fill's.
    stroked: bool,
}

/// One item's outline as it was last flattened, and everything that decided those numbers.
#[derive(Debug)]
///
/// Held so that a frame which draws the same geometry in the same place does not flatten it again.
/// **That is most frames on which anything vector is on the screen**: a menu opening over a drawing
/// damages the drawing, so the drawing is emitted and collected again for every frame of the
/// menu's animation, and every one of those flattens outlines identical to the last.
struct Flattened {
    /// The path it came from, compared by allocation.
    ///
    /// A `VectorId` is stable across frames for one item, which is what makes it a key; it says
    /// nothing about whether the geometry under it changed. The path is behind an `Arc` and what
    /// is behind one never changes, so two handles onto one allocation are two handles onto the
    /// same outline.
    path: Arc<kurbo::BezPath>,
    /// The matrix the segments below already carry.
    ///
    /// Flattening subdivides in the path's own space and transforms the points it produces, so a
    /// different placement is different numbers. Keeping the placement rather than re-transforming
    /// cached points makes a hit exactly the bytes the old code produced.
    placement: [f64; 6],
    /// The stroke style it was expanded under, for the stroked outline.
    stroke: Option<kurbo::Stroke>,
    /// The outline itself.
    segments: Vec<Segment>,
    /// The frame this was last wanted on, so an item that has gone stops being held.
    used: u64,
}
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
    /// Every outline the fragment stage walks: each band's own segments, then each clip run's.
    outlines: Vec<Segment>,
    /// Per band: where its own segments start in `outlines`, and how many there are.
    bands: Vec<[u32; 4]>,
    /// One item's flattened outline, before it is cut into bands. Held to allocate nothing per item.
    flattened: Vec<Segment>,
    /// What each item's outline flattened to last time it was asked for. See [`Flattened`].
    outlines_of: rustc_hash::FxHashMap<OutlineKey, Flattened>,
    /// Which frame this is, for deciding what in the cache above is still wanted.
    frames: u64,
    /// How many segments each band of the item being cut holds, then where each band's start. Held
    /// for the same reason.
    tally: Vec<u32>,
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
    /// What the last frame cost and could not do, over every round it rasterised.
    last: Rasterised,
    /// How many layers the last frame's passes needed.
    depth: u32,
    /// How many rounds the last frame's passes needed.
    rounds: usize,
    /// How much scratch one round may hold.
    capacity: Capacity,
}

/// Where one item's bands are, and what they cover.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Banding {
    /// The first band, in the band table.
    first: u32,
    /// How many bands there are; zero for a shape with nothing to fill.
    count: u32,
    /// The pixel boundary the first band starts at.
    top: f32,
    /// How many pixels tall one band is, which is a whole number and at least one.
    tall: f32,
}

/// The first and last band a segment reaches, both inclusive.
///
/// A segment belongs to every band its y-extent touches: one that ends inside a band and one that
/// crosses it whole are both crossings a ray may meet. A segment outside the bands altogether is
/// clamped into the nearest, where it crosses no sample row and so counts for nothing.
fn band_span(segment: Segment, top: f32, tall: f32, count: usize) -> (usize, usize) {
    let last = count.saturating_sub(1) as isize;
    let of = |y: f32| (((y - top) / tall).floor() as isize).clamp(0, last) as usize;
    (
        of(segment[1].min(segment[3])),
        of(segment[1].max(segment[3])),
    )
}

/// The device-side copies of the host arrays.
#[derive(Debug)]
struct Buffers {
    /// The items.
    items: Storage,
    /// Every outline the fragment stage walks.
    outlines: Storage,
    /// The clip runs.
    runs: Storage,
    /// Per band: where its own segments start, and how many.
    bands: Storage,
}

impl CoverageRaster {
    /// The most bands one shape may have, so a very tall shape cannot produce an unbounded table.
    ///
    /// A band is one pixel tall until a shape is taller than this, which no shape on a display of
    /// this generation is; past it the bands grow rather than the table.
    const MAX_BANDS: usize = 4096;

    /// A rasteriser on `gpu`, sized for a surface of `width` by `height` device pixels.
    pub fn new(gpu: &Arc<Gpu>, width: u32, height: u32) -> Self {
        let mut scratch = Scratch::new();
        scratch.ensure(gpu, width.max(1), height.max(1), Scratch::LAYERS);
        Self {
            pipelines: Pipelines::new(gpu),
            scratch,
            outlines: Vec::new(),
            bands: Vec::new(),
            flattened: Vec::new(),
            outlines_of: rustc_hash::FxHashMap::default(),
            frames: 0,
            tally: Vec::new(),
            runs: Vec::new(),
            items: Vec::new(),
            spans: Vec::new(),
            regions: Vec::new(),
            layered: Vec::new(),
            buffers: Buffers {
                items: Storage::new(gpu, "zgui.vector.coverage.items"),
                outlines: Storage::new(gpu, "zgui.vector.coverage.outlines"),
                runs: Storage::new(gpu, "zgui.vector.coverage.runs"),
                bands: Storage::new(gpu, "zgui.vector.coverage.bands"),
            },
            gpu: Arc::clone(gpu),
            last: Rasterised::default(),
            depth: 0,
            rounds: 0,
            capacity: Capacity::DEFAULT,
        }
    }

    /// This rasteriser with `capacity` of scratch per round.
    ///
    /// The layers are clamped to [`Scratch::MAX_LAYERS`].
    #[must_use]
    pub fn with_capacity(self, capacity: Capacity) -> Self {
        Self {
            capacity: Capacity {
                layers: capacity.layers.clamp(1, Scratch::MAX_LAYERS),
                ..capacity
            },
            ..self
        }
    }

    /// What the last frame cost, and what it could not do.
    ///
    /// A round that a frame rasterises again counts again.
    pub fn last_frame(&self) -> Rasterised {
        self.last
    }

    /// How many scratch layers the last frame's passes needed in its fullest round.
    ///
    /// The frame's own demand, which follows the area of its passes. The allocated layers
    /// ([`layers`](Self::layers)) never fall below a floor and follow the demand down slowly.
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// How many rounds the last frame's passes needed.
    pub fn rounds(&self) -> usize {
        self.rounds
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
        // From the pass's region on the surface to its bin in the layer.
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
            // The residual is a clip chain in device space, so it moves into the bin with the pass
            // and never goes through the item's own transform.
            let clip_first = self.runs.len();
            for shape in &shapes {
                let start = self.outlines.len();
                geometry::flatten(shape, shift, geometry::TOLERANCE, &mut self.outlines);
                self.runs.push(Run::new(start, self.outlines.len() - start));
                self.last.segments += (self.outlines.len() - start) as u32;
                self.last.clip_layers += 1;
            }

            let placement = shift * self.transform_of(item, frame) * item.placement;
            // A path in path space flattened to a tenth of a device pixel, however far the
            // placement scales it.
            let tolerance = geometry::tolerance(placement);
            // The item's own shape clips are in the item's path space, so unlike the residual
            // they go through the item's placement and transform: a clipped drawing that is
            // rotated has its clip rotated with it.
            for clip in &item.clips {
                let start = self.outlines.len();
                geometry::flatten(&clip.path, placement, tolerance, &mut self.outlines);
                self.runs.push(Run::of(
                    start,
                    self.outlines.len() - start,
                    clip.rule == peniko::Fill::EvenOdd,
                ));
                self.last.segments += (self.outlines.len() - start) as u32;
                self.last.clip_layers += 1;
            }
            let clip_count = self.runs.len() - clip_first;

            // The ink is recorded relative to its pass's region, and the bin is where that region
            // is in the layer. A pixel of slack on each side, except past the edge of a clamped
            // pass: the pass beside it in the layer starts there.
            let mut bounds = [
                (origin.x + planned.ink.origin.x) as f32 - 1.0,
                (origin.y + planned.ink.origin.y) as f32 - 1.0,
                planned.ink.size.width as f32 + 2.0,
                planned.ink.size.height as f32 + 2.0,
            ];
            if pass.clamped {
                bounds = clamped(bounds, pass.raster_region);
            }
            let mut painted = false;
            if let Some(color) = flat(item.fill, frame.paints) {
                let _reused = Self::outline_of(
                    &mut self.flattened,
                    &mut self.outlines_of,
                    item,
                    None,
                    placement,
                    tolerance,
                    self.frames,
                );
                let band = self.band(bounds);
                self.items.push(Item {
                    bounds,
                    viewport: extent,
                    color,
                    control: [
                        band.first as f32,
                        band.count as f32,
                        f32::from(u8::from(item.fill_rule == peniko::Fill::EvenOdd)),
                        clip_first as f32,
                    ],
                    bands: [clip_count as f32, band.top, band.tall, 0.0],
                });
                painted = true;
            }
            if let Some(stroke) = item.stroke.as_ref()
                && let Some(color) = flat(Some(stroke.paint), frame.paints)
            {
                let _reused = Self::outline_of(
                    &mut self.flattened,
                    &mut self.outlines_of,
                    item,
                    Some(&stroke.style),
                    placement,
                    tolerance,
                    self.frames,
                );
                let band = self.band(bounds);
                self.items.push(Item {
                    bounds,
                    viewport: extent,
                    color,
                    // A stroke's outline is always filled by the non-zero rule whatever the fill
                    // rule of the shape it came from: the outline is a boundary, not a region the
                    // author wrote a rule for.
                    control: [band.first as f32, band.count as f32, 0.0, clip_first as f32],
                    bands: [clip_count as f32, band.top, band.tall, 0.0],
                });
                painted = true;
            }
            if !painted {
                self.last.unpaintable += 1;
            }
        }
        (first, self.items.len() as u32 - first)
    }

    /// Fills `into` with one item's outline, flattening it only where the last one still holds.
    ///
    /// A hit is exactly the bytes flattening would have produced, because what is held is the outline
    /// *after* the placement was applied rather than before it — so this changes what a frame costs and
    /// never what it draws.
    ///
    /// Three things have to match for the held outline to still stand: the path's allocation, the
    /// placement it was transformed by, and, for a stroke, the style it was expanded under. Anything
    /// else is a different outline and is flattened again.
    fn outline_of(
        into: &mut Vec<Segment>,
        held: &mut rustc_hash::FxHashMap<OutlineKey, Flattened>,
        item: &VectorItem,
        stroke: Option<&kurbo::Stroke>,
        placement: Affine,
        tolerance: f64,
        frame: u64,
    ) -> bool {
        let key = OutlineKey {
            id: item.id,
            stroked: stroke.is_some(),
        };
        let placement = placement.as_coeffs();
        into.clear();
        if let Some(entry) = held.get_mut(&key)
            && Arc::ptr_eq(&entry.path, &item.path)
            && entry.placement == placement
            && entry.stroke.as_ref() == stroke
        {
            entry.used = frame;
            into.extend_from_slice(&entry.segments);
            return true;
        }
        match stroke {
            Some(style) => {
                geometry::flatten_stroke(
                    &item.path,
                    style,
                    Affine::new(placement),
                    tolerance,
                    into,
                );
            }
            None => geometry::flatten(&item.path, Affine::new(placement), tolerance, into),
        }
        held.insert(
            key,
            Flattened {
                path: Arc::clone(&item.path),
                placement,
                stroke: stroke.cloned(),
                segments: into.clone(),
                used: frame,
            },
        );
        false
    }

    /// Drops the outlines nothing has asked for in a while.
    ///
    /// Not "anything this frame did not draw": a frame that repaints one corner collects only what
    /// is in that corner, and evicting the rest would flatten the whole screen again on the next
    /// frame that touches it. So an outline is held for a couple of seconds' worth of frames after
    /// it was last wanted, which keeps a scrolled-away drawing warm and lets a closed document go.
    ///
    /// Swept occasionally rather than every frame, because the sweep walks the whole map and the
    /// thing being paid for here is per-frame work.
    fn forget_stale_outlines(&mut self) {
        /// How many frames an outline is held after the last frame that wanted it.
        const KEPT_FOR: u64 = 120;
        /// How often the sweep runs.
        const EVERY: u64 = 64;

        if !self.frames.is_multiple_of(EVERY) {
            return;
        }
        let frames = self.frames;
        self.outlines_of
            .retain(|_, held| frames.saturating_sub(held.used) < KEPT_FOR);
    }

    /// Cuts the flattened outline into horizontal bands over `bounds`.
    ///
    /// A fragment then walks the handful of segments in its own band instead of every segment of
    /// the shape, which is the same answer reached by less arithmetic: a segment that does not
    /// cross the sample's row cannot cross the ray cast from it. Sixteen samples a pixel over a
    /// display-sized glyph is tens of millions of segment tests a frame, and this is what takes the
    /// multiplier out of it.
    ///
    /// The bands are a whole number of pixels tall and start on a pixel boundary, so a pixel lies
    /// wholly inside one of them. That is what lets a fragment find its band once and walk it once
    /// for all sixteen of its samples.
    ///
    /// Each band holds a copy of its own segments rather than indices into a shared table. A
    /// segment that spans several bands is written once for each, which costs the host a little
    /// memory and saves the device one texture fetch per segment per pixel.
    fn band(&mut self, bounds: [f32; 4]) -> Banding {
        // The pixel boundary at or above the box, because a band boundary has to be one too.
        let top = bounds[1].floor();
        let first = self.bands.len() as u32;
        self.last.segments += self.flattened.len() as u32;
        let height = bounds[1] + bounds[3] - top;
        // `is_finite` as well as the sign: a height that came out NaN would otherwise divide into
        // a band count of nothing, and the shape would be tested against no segments at all.
        if self.flattened.is_empty() || !height.is_finite() || height <= 0.0 {
            return Banding {
                first,
                count: 0,
                top,
                tall: 1.0,
            };
        }
        // One pixel a band, until the shape is taller than the table may be; then whole pixels
        // still, but more of them each.
        let tall = (height / Self::MAX_BANDS as f32).ceil().max(1.0);
        let count = ((height / tall).ceil() as usize).clamp(1, Self::MAX_BANDS);

        // Counted first and placed second, so that each band's segments end up next to each other
        // without a list per band. `tally` holds the count of each band, then where each starts.
        self.tally.clear();
        self.tally.resize(count, 0);
        for index in 0..self.flattened.len() {
            let (low, high) = band_span(self.flattened[index], top, tall, count);
            for slot in low..=high {
                self.tally[slot] += 1;
            }
        }
        let base = self.outlines.len();
        let mut running = 0;
        for slot in 0..count {
            let held = self.tally[slot];
            self.tally[slot] = running;
            self.bands.push([base as u32 + running, held, 0, 0]);
            running += held;
        }
        self.outlines.resize(base + running as usize, [0.0; 4]);
        for index in 0..self.flattened.len() {
            let segment = self.flattened[index];
            let (low, high) = band_span(segment, top, tall, count);
            for slot in low..=high {
                self.outlines[base + self.tally[slot] as usize] = segment;
                self.tally[slot] += 1;
            }
        }
        Banding {
            first,
            count: count as u32,
            top,
            tall,
        }
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

    /// Sorts the passes of one round into the layer each one went into.
    fn group(&mut self, plan: &VectorPlan, round: core::ops::Range<usize>) {
        let layers = (self.scratch.layers() as usize).max(1);
        for bucket in &mut self.layered {
            bucket.clear();
        }
        self.layered.resize_with(layers, Vec::new);
        for (span, index) in round.enumerate() {
            if let Some(bucket) = self.layered.get_mut(plan.passes[index].target.0 as usize) {
                bucket.push(span);
            }
        }
    }

    /// Records every pass of the frame into one encoder and submits it.
    ///
    /// One accumulation pass and one resolve per *layer*, not per pass: the passes sharing a layer
    /// have bins that do not meet, so their outlines accumulate side by side and one draw converts
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
                        resource: self.buffers.outlines.binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.buffers.runs.binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.buffers.bands.binding(),
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
    fn backend(&self) -> zgui_render::VectorBackend {
        zgui_render::VectorBackend::Coverage
    }

    fn plan(&mut self, passes: &ScenePassPlan) -> VectorPlan {
        if passes.is_empty() {
            return VectorPlan::empty();
        }
        // A frame plans once and can prepare a round more than once, so the frame starts here.
        self.last = Rasterised::default();
        let mut plan = VectorPlan::resourcing(passes);
        // Each pass gets a bin of its own size in some layer, so passes share a layer whatever they
        // cover on the surface. A frame larger than one scratch is cut into rounds, and each round
        // is rasterised after the composites of the round before it have read the layers.
        self.regions.clear();
        self.regions
            .extend(passes.passes.iter().map(|planned| planned.region));
        let layering = Layering::of(&self.regions, self.capacity);
        self.depth = layering.layers();
        self.rounds = layering.rounds();
        for (index, planned) in passes.passes.iter().enumerate() {
            let Some(placement) = layering.placement(index) else {
                continue;
            };
            plan.passes.push(VectorPass {
                region: planned.region,
                raster_region: placement.bin,
                target: placement.target,
                round: placement.round,
                items: planned.items.clone(),
                clip: planned.clip,
                instanced: planned.instanced,
                clamped: planned.clamped || planned.overhangs,
            });
        }
        let (width, height) = layering.extent();
        self.scratch
            .ensure(&self.gpu, width, height, layering.layers());
        plan
    }

    fn clear_targets(&mut self, plan: &VectorPlan, round: usize) {
        let mut layers: Vec<u32> = plan.passes[plan.round(round)]
            .iter()
            .filter(|pass| pass.target != VectorTarget::NONE)
            .map(|pass| pass.target.0 as u32)
            .collect();
        layers.sort_unstable();
        layers.dedup();
        self.scratch.clear(&self.gpu, &layers);
    }

    fn prepare(&mut self, frame: &mut VectorFrame<'_>) -> Result<(), VectorError> {
        self.frames = self.frames.wrapping_add(1);
        self.forget_stale_outlines();
        self.last = Rasterised::default();
        self.outlines.clear();
        self.bands.clear();
        self.runs.clear();
        self.items.clear();
        self.spans.clear();
        if frame.is_empty() {
            return Ok(());
        }
        let round = frame.passes();
        {
            let _stage = tracing::debug_span!("cov.collect").entered();
            for index in round.clone() {
                let pass = frame.plan.passes[index].clone();
                let span = self.collect(frame, &pass);
                self.spans.push(span);
            }
            self.group(frame.plan, round);
        }
        {
            let _stage = tracing::debug_span!(
                "cov.upload",
                items = self.items.len(),
                outlines = self.outlines.len(),
                bands = self.bands.len(),
                runs = self.runs.len()
            )
            .entered();
            self.buffers.items.upload(&self.gpu, &self.items);
            self.buffers.outlines.upload(&self.gpu, &self.outlines);
            self.buffers.runs.upload(&self.gpu, &self.runs);
            self.buffers.bands.upload(&self.gpu, &self.bands);
        }
        {
            let _stage = tracing::debug_span!("cov.record").entered();
            self.record()?;
        }
        if self.last.unclippable > 0 {
            tracing::warn!(
                items = self.last.unclippable,
                "vector items left undrawn because a residual clip had no shape to apply"
            );
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
                + self.buffers.outlines.capacity()
                + self.buffers.runs.capacity()
                + self.buffers.bands.capacity(),
            ..MemoryReport::ZERO
        }
    }

    fn release_idle_resources(&mut self) -> u64 {
        let mut freed = self.scratch.release();
        freed += self.buffers.items.shrink(&self.gpu);
        freed += self.buffers.outlines.shrink(&self.gpu);
        freed += self.buffers.runs.shrink(&self.gpu);
        freed += self.buffers.bands.shrink(&self.gpu);
        self.items.clear();
        self.items.shrink_to_fit();
        self.outlines.clear();
        self.outlines.shrink_to_fit();
        self.runs.clear();
        self.runs.shrink_to_fit();
        self.flattened.shrink_to_fit();
        self.tally.shrink_to_fit();
        self.bands.shrink_to_fit();
        freed
    }
}

impl VectorSource for CoverageRaster {
    fn view(&self, target: VectorTarget) -> Option<&wgpu::TextureView> {
        self.scratch.straight(target.0 as u32)
    }
}

/// `bounds`, as x, y, width and height, kept inside `region`.
fn clamped(bounds: [f32; 4], region: Rect<i32, Device>) -> [f32; 4] {
    let left = bounds[0].max(region.left() as f32);
    let top = bounds[1].max(region.top() as f32);
    let right = (bounds[0] + bounds[2]).min(region.right() as f32);
    let bottom = (bounds[1] + bounds[3]).min(region.bottom() as f32);
    [left, top, (right - left).max(0.0), (bottom - top).max(0.0)]
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

#[cfg(test)]
mod outlines {
    //! What the held outlines answer, which needs no device: flattening is arithmetic.

    use std::sync::Arc;

    use kurbo::{Affine, BezPath, Stroke};
    use zgui_scene::{PaintRef, VectorId, VectorItem};

    use super::{CoverageRaster, Flattened, OutlineKey, Segment, geometry};

    /// A path with curves in it, so that flattening has something to do.
    fn item(id: u32) -> VectorItem {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.curve_to((10.0, 0.0), (20.0, 10.0), (20.0, 20.0));
        path.curve_to((20.0, 30.0), (10.0, 40.0), (0.0, 40.0));
        path.close_path();
        VectorItem::filled(VectorId(id), Arc::new(path), PaintRef::NONE)
    }

    /// Flattens one item and says whether the held outline was reused.
    fn flatten(
        into: &mut Vec<Segment>,
        held: &mut rustc_hash::FxHashMap<OutlineKey, Flattened>,
        item: &VectorItem,
        stroke: Option<&Stroke>,
        placement: Affine,
    ) -> bool {
        CoverageRaster::outline_of(into, held, item, stroke, placement, geometry::TOLERANCE, 1)
    }

    #[test]
    fn the_same_geometry_in_the_same_place_is_flattened_once() {
        let mut held = rustc_hash::FxHashMap::default();
        let mut into = Vec::new();
        let item = item(1);

        assert!(
            !flatten(&mut into, &mut held, &item, None, Affine::IDENTITY),
            "nothing was held yet"
        );
        let first = into.clone();
        assert!(!first.is_empty(), "the path flattened to something");

        assert!(
            flatten(&mut into, &mut held, &item, None, Affine::IDENTITY),
            "the second ask is the same path in the same place"
        );
        assert_eq!(
            into, first,
            "a hit answers exactly what flattening answered, so a frame draws the same picture"
        );
    }

    #[test]
    fn a_different_placement_is_a_different_outline() {
        let mut held = rustc_hash::FxHashMap::default();
        let mut into = Vec::new();
        let item = item(1);

        flatten(&mut into, &mut held, &item, None, Affine::IDENTITY);
        assert!(
            !flatten(
                &mut into,
                &mut held,
                &item,
                None,
                Affine::translate((3.0, 0.0))
            ),
            "the segments carry the placement, so moving the item is different numbers"
        );
    }

    #[test]
    fn a_stroke_is_held_apart_from_the_fill_and_follows_its_style() {
        let mut held = rustc_hash::FxHashMap::default();
        let mut into = Vec::new();
        let item = item(1);
        let thin = Stroke::new(1.0);
        let thick = Stroke::new(4.0);

        flatten(&mut into, &mut held, &item, None, Affine::IDENTITY);
        assert!(
            !flatten(&mut into, &mut held, &item, Some(&thin), Affine::IDENTITY),
            "a stroke's outline is not the fill's, so the fill's entry does not answer for it"
        );
        assert!(flatten(
            &mut into,
            &mut held,
            &item,
            Some(&thin),
            Affine::IDENTITY
        ));
        assert!(
            !flatten(&mut into, &mut held, &item, Some(&thick), Affine::IDENTITY),
            "a wider stroke expands to a different outline"
        );
        assert!(
            flatten(&mut into, &mut held, &item, None, Affine::IDENTITY),
            "and the fill it was held beside is still there"
        );
    }

    #[test]
    fn geometry_that_changed_under_one_identity_is_flattened_again() {
        // A `VectorId` is stable for an item across frames and says nothing about whether what it
        // draws changed. What is behind an `Arc` never changes, so the allocation is the answer.
        let mut held = rustc_hash::FxHashMap::default();
        let mut into = Vec::new();
        let first = item(1);
        let second = item(1);

        flatten(&mut into, &mut held, &first, None, Affine::IDENTITY);
        assert!(
            !flatten(&mut into, &mut held, &second, None, Affine::IDENTITY),
            "the same identity over a different path is a different outline"
        );
    }
}
