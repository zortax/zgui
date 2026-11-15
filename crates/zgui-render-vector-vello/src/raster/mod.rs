//! The rasteriser: pre-clear, encode, rasterise, one rasterisation per scratch layer.

pub mod cached;
pub mod encode;
pub mod paint;
pub mod scratch;

use std::sync::Arc;

use vello::{AaConfig, RenderParams, peniko::Color as PenikoColor};
use zgui_geom::{Device, Rect};
use zgui_profile::{Counter, counter};
use zgui_render::{
    Capacity, Layering, MemoryReport, VectorError, VectorFrame, VectorPass, VectorPlan,
    VectorRaster, VectorTarget,
};
use zgui_render_wgpu::Gpu;
use zgui_render_wgpu::frame::vector::VectorSource;
use zgui_scene::ScenePassPlan;

use crate::device::{Shared, for_gpu};
use crate::raster::cached::Encodings;
use crate::raster::encode::Encoded;
use crate::raster::scratch::Scratch;

/// A vector rasteriser backed by a compute-shader path renderer.
///
/// One of these per window; the renderer underneath is one per *device*, because its fixed buffers
/// are measured in hundreds of megabytes and one copy per window would be a device's whole budget.
pub struct VelloRaster {
    /// The device it draws on.
    gpu: Arc<Gpu>,
    /// The renderer shared by everything on that device.
    shared: Shared,
    /// The texture batches land in.
    scratch: Scratch,
    /// One reusable scene per layer, so nothing reallocates between frames.
    scenes: Vec<vello::Scene>,
    /// The regions the layering is computed from, kept so that a frame allocates nothing for it.
    regions: Vec<Rect<i32, Device>>,
    /// Which passes went into which layer, kept for the same reason.
    layered: Vec<Vec<usize>>,
    /// Each item's encoded form, kept across frames.
    encodings: Encodings,
    /// What the last frame's encoding cost and could not do, over every round it rasterised.
    last: Encoded,
    /// How many passes the last frame rasterised, over every round it rasterised.
    passes: u32,
    /// How many layers the last frame's passes needed.
    depth: u32,
    /// How many rounds the last frame's passes needed.
    rounds: usize,
    /// How much scratch one round may hold.
    capacity: Capacity,
}

/// The coverage method every rasterisation asks for.
///
/// The one the renderer is built with, and therefore the only one it can run. Analytic area
/// coverage is faster and re-uploads no sample-mask table; the multisampled alternative was
/// compared against it on the content that provokes conflation artefacts and showed nothing area
/// coverage hides, so it is no longer built. See `SharedRenderer::new`.
const ANTIALIASING: AaConfig = AaConfig::Area;

impl std::fmt::Debug for VelloRaster {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VelloRaster")
            .field("scratch", &self.scratch.extent())
            .field("encodings", &self.encodings)
            .field("last", &self.last)
            .finish_non_exhaustive()
    }
}

impl VelloRaster {
    /// A rasteriser on `gpu`, sized for a surface of `width` by `height` device pixels.
    ///
    /// # Errors
    ///
    /// [`VectorError::Device`] when the device cannot run what a path renderer needs — no compute
    /// shaders, or no writable storage textures. That is a real device rather than a hypothetical
    /// one, which is why there is a second rasteriser to bind instead of a panic here.
    pub fn new(gpu: &Arc<Gpu>, width: u32, height: u32) -> Result<Self, VectorError> {
        let _ = (width, height);
        let shared = for_gpu(gpu).map_err(|error| VectorError::Device {
            detail: error.to_string(),
        })?;
        // Unallocated. The scratch is sized from what a frame's passes actually reach — the far
        // corner anything is drawn at, not the surface — and `plan` asks for that on every frame
        // that has anything to rasterise. Sizing it here instead spent the surface four times over
        // on the frame a rasteriser was built, which is fifty-nine megabytes at 2560 by 1440 for a
        // document whose vector content may be one icon in a corner.
        Ok(Self {
            gpu: Arc::clone(gpu),
            shared,
            scratch: Scratch::new(),
            scenes: (0..Scratch::LAYERS).map(|_| vello::Scene::new()).collect(),
            regions: Vec::new(),
            layered: Vec::new(),
            encodings: Encodings::new(),
            last: Encoded::default(),
            passes: 0,
            depth: 0,
            rounds: 0,
            capacity: Capacity::DEFAULT,
        })
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

    /// Which coverage method every rasterisation asks for.
    pub fn antialiasing(&self) -> AaConfig {
        ANTIALIASING
    }

    /// What the last frame's encoding cost, and what it could not do.
    ///
    /// A round that a frame rasterises again counts again.
    pub fn last_frame(&self) -> Encoded {
        self.last
    }

    /// How many passes the last frame rasterised.
    ///
    /// A round that a frame rasterises again counts again.
    pub fn passes(&self) -> u32 {
        self.passes
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

    /// How many item encodings are held, and how the last frame's lookups went.
    pub fn cache(&self) -> (usize, (u64, u64)) {
        (self.encodings.len(), self.encodings.counts())
    }

    /// How many scratch layers are allocated.
    pub fn layers(&self) -> u32 {
        self.scratch.layers()
    }

    /// The extent every scratch layer is allocated at.
    pub fn extent(&self) -> (u32, u32) {
        self.scratch.extent()
    }

    /// Sorts the passes of one round into the layer each one went into.
    ///
    /// One rasterisation per layer is the whole point of sharing: a frame of five hundred passes
    /// costs one rasterisation, where one per pass cost five hundred of them and each one is a
    /// recording, a submission and its own fixed overhead.
    fn group(&mut self, plan: &VectorPlan, round: core::ops::Range<usize>) {
        let layers = (self.scratch.layers() as usize).max(1);
        for bucket in &mut self.layered {
            bucket.clear();
        }
        self.layered.resize_with(layers, Vec::new);
        self.scenes.resize_with(layers, vello::Scene::new);
        for index in round {
            if let Some(bucket) = self.layered.get_mut(plan.passes[index].target.0 as usize) {
                bucket.push(index);
            }
        }
    }

    /// Encodes and rasterises one round of a frame's passes.
    fn prepare_frame(&mut self, frame: &mut VectorFrame<'_>) -> Result<(), VectorError> {
        if frame.is_empty() {
            return Ok(());
        }
        self.group(frame.plan, frame.passes());
        let (hits, misses) = self.encodings.counts();
        let before = self.last;
        let mut renderer = self.shared.lock();
        for layer in 0..self.layered.len() {
            // One rasterisation for the whole layer, because everything in it is one picture: each
            // pass sharing it has a bin of its own, so their coverage is disjoint here.
            let scene = &mut self.scenes[layer];
            scene.reset();
            let mut width = 0;
            let mut height = 0;
            for &index in &self.layered[layer] {
                let pass = &frame.plan.passes[index];
                let (region_width, region_height) = encode::extent(pass.raster_region);
                if region_width == 0 || region_height == 0 {
                    continue;
                }
                let encoded = encode::pass(scene, &mut self.encodings, frame, pass);
                self.last.clip_layers += encoded.clip_layers;
                self.last.unclippable += encoded.unclippable;
                self.last.unpaintable += encoded.unpaintable;
                self.last.flattened_transforms += encoded.flattened_transforms;
                width = width.max(pass.raster_region.right().max(0) as u32);
                height = height.max(pass.raster_region.bottom().max(0) as u32);
                self.passes += 1;
            }
            if width == 0 || height == 0 {
                continue;
            }
            let Some(view) = self.scratch.view(layer as u32) else {
                continue;
            };
            let shared = &self.shared;
            let gpu = &self.gpu;
            let antialiasing = ANTIALIASING;
            shared
                .measuring(gpu, || {
                    renderer.render_to_texture(
                        gpu.device(),
                        gpu.queue(),
                        scene,
                        view,
                        &RenderParams {
                            // Transparent, always. Anything else would make the parts of the layer
                            // a pass did not paint composite as a colour rather than as nothing,
                            // which is exactly what makes coalescing items into one pass sound.
                            base_color: PenikoColor::TRANSPARENT,
                            // As far as the furthest pass of this layer reaches, and no further: the
                            // rasterisation costs its own area whether anything is drawn in it or
                            // not, so a layer holding one small pass must not cost a whole surface.
                            width,
                            height,
                            antialiasing_method: antialiasing,
                        },
                    )
                })
                .map_err(|error| VectorError::Device {
                    detail: error.to_string(),
                })?;
            counter::bump(Counter::VelloRenders);
        }
        drop(renderer);
        let (hits_now, misses_now) = self.encodings.counts();
        counter::add(Counter::VectorEncodeHits, hits_now - hits);
        counter::add(Counter::VectorEncodeMisses, misses_now - misses);
        let unclippable = self.last.unclippable - before.unclippable;
        if unclippable > 0 {
            tracing::warn!(
                items = unclippable,
                "vector items left undrawn because a residual clip had no shape to apply"
            );
        }
        let unpaintable = self.last.unpaintable - before.unpaintable;
        if unpaintable > 0 {
            tracing::debug!(
                items = unpaintable,
                "vector items left undrawn because nothing here paints what they asked for"
            );
        }
        Ok(())
    }
}

impl VectorRaster for VelloRaster {
    fn backend(&self) -> zgui_render::VectorBackend {
        zgui_render::VectorBackend::Vello
    }

    fn plan(&mut self, passes: &ScenePassPlan) -> VectorPlan {
        // The one question worth asking before anything else: a frame with no surviving path runs no
        // rasterisation at all, because a deliberately empty pass over a whole surface is tens of
        // microseconds of processor time and several times that in latency.
        if passes.is_empty() {
            return VectorPlan::empty();
        }
        // A frame plans once and can prepare a round more than once, so the frame starts here. An
        // encoding the frame before did not use ages here.
        self.encodings.end_frame();
        self.last = Encoded::default();
        self.passes = 0;
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
        self.prepare_frame(frame)
    }

    fn memory(&self) -> MemoryReport {
        MemoryReport {
            // Two budgets, reported separately on purpose. The first does not scale with anything at
            // all and is by far the larger; the second scales with the surface and with how much is
            // rasterised at once. One number would hide whichever of the two was being spent.
            fixed: self.shared.fixed_bytes(),
            scratch: self.scratch.bytes(),
            ..MemoryReport::ZERO
        }
    }

    fn release_idle_resources(&mut self) -> u64 {
        self.scratch.release()
    }
}

impl VectorSource for VelloRaster {
    fn view(&self, target: VectorTarget) -> Option<&wgpu::TextureView> {
        self.scratch.view(target.0 as u32)
    }
}
