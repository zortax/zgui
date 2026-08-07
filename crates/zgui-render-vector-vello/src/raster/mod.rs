//! The rasteriser: pre-clear, encode, rasterise, one rasterisation per scratch layer.

pub mod cached;
pub mod encode;
pub mod paint;
pub mod scratch;

use std::sync::Arc;

use vello::{AaConfig, RenderParams, peniko::Color as PenikoColor};
use zgui_geom::{Device, Rect};
use zgui_render::{
    Layering, MemoryReport, VectorError, VectorFrame, VectorPass, VectorPlan, VectorRaster,
    VectorTarget,
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
    /// What the last frame's encoding cost and could not do.
    last: Encoded,
    /// How many passes the last frame rasterised.
    passes: u32,
    /// How many layers the last frame's passes needed.
    depth: u32,
    /// The coverage method every rasterisation asks for.
    antialiasing: AaConfig,
}

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
        let shared = for_gpu(gpu).map_err(|error| VectorError::Device {
            detail: error.to_string(),
        })?;
        let mut scratch = Scratch::new();
        scratch.ensure(gpu, width.max(1), height.max(1), Scratch::LAYERS);
        Ok(Self {
            gpu: Arc::clone(gpu),
            shared,
            scratch,
            scenes: (0..Scratch::LAYERS).map(|_| vello::Scene::new()).collect(),
            regions: Vec::new(),
            layered: Vec::new(),
            encodings: Encodings::new(),
            last: Encoded::default(),
            passes: 0,
            depth: 0,
            antialiasing: AaConfig::Area,
        })
    }

    /// Which coverage method every rasterisation asks for.
    ///
    /// Analytic area coverage is the default: it is faster, and it does not re-upload a sample-mask
    /// table on every rasterisation. The multisampled alternative exists so the two can be compared
    /// on real content — overlapping strokes, a rounded icon, a self-intersecting path — rather than
    /// argued about, because the conflation artefacts area coverage can produce are a property of
    /// the content and not of the algorithm alone.
    pub fn antialiasing(&self) -> AaConfig {
        self.antialiasing
    }

    /// Sets the coverage method.
    pub fn set_antialiasing(&mut self, antialiasing: AaConfig) {
        self.antialiasing = antialiasing;
    }

    /// What the last frame's encoding cost, and what it could not do.
    pub fn last_frame(&self) -> Encoded {
        self.last
    }

    /// How many passes the last frame rasterised.
    pub fn passes(&self) -> u32 {
        self.passes
    }

    /// How many scratch layers the last frame's passes needed.
    ///
    /// The frame's own demand, which is what says how much of it overlapped — not how many layers
    /// are allocated, which never falls below a floor and follows the demand down only slowly.
    pub fn depth(&self) -> u32 {
        self.depth
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

    /// Sorts the passes that were given a layer into the layer each one went into.
    ///
    /// One rasterisation per layer is the whole point of sharing: a frame of five hundred passes
    /// that nowhere overlap costs one rasterisation, where one per pass cost five hundred of them
    /// and each one is a recording, a submission and its own fixed overhead.
    fn group(&mut self, passes: &[VectorPass]) {
        let layers = (self.scratch.layers() as usize).max(1);
        for bucket in &mut self.layered {
            bucket.clear();
        }
        self.layered.resize_with(layers, Vec::new);
        self.scenes.resize_with(layers, vello::Scene::new);
        for (index, pass) in passes.iter().enumerate() {
            if let Some(bucket) = self.layered.get_mut(pass.target.0 as usize) {
                bucket.push(index);
            }
        }
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
        let mut plan = VectorPlan::resourcing(passes);
        // Layers are shared by passes that do not meet on the surface. Every pass of a frame is
        // still rasterised before any of them is composited, so a layer holds its passes' coverage
        // until their composites have read it — and because a layer is in device coordinates,
        // passes that do not overlap there do not overlap in it either.
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
        self.last = Encoded::default();
        self.passes = 0;
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
        self.group(&frame.plan.passes[..prepared]);
        let mut renderer = self.shared.lock();
        for layer in 0..self.layered.len() {
            // One rasterisation for the whole layer, because everything in it is one picture: the
            // passes sharing it are disjoint on the surface, so their coverage is disjoint here.
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
            let antialiasing = self.antialiasing;
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
        }
        drop(renderer);
        if prepared < frame.plan.passes.len() {
            // More passes stacked over one point than there are layers to keep them apart. Reporting
            // it is what makes this frame's vector content missing rather than jumbled: the
            // alternative is two overlapping passes on one layer, and then one composite draws the
            // other's paths.
            return Err(VectorError::OutOfCapacity {
                detail: format!(
                    "a frame planned {} passes and {} of them could be given one of {} layers",
                    frame.plan.passes.len(),
                    prepared,
                    Scratch::MAX_LAYERS
                ),
                // Every earlier pass has a layer to itself or shares one with a pass it does not
                // touch, and is safe to composite. Only this one and the ones after it have nowhere
                // to go.
                prepared,
            });
        }
        // Everything drawn this frame is current; anything else is only worth keeping while there
        // is room for it.
        let drawn: std::collections::HashSet<zgui_scene::VectorId> = frame
            .plan
            .items
            .iter()
            .filter_map(|planned| frame.items.get(planned.item).map(|item| item.id))
            .collect();
        self.encodings
            .retain_if_over_capacity(|id| drawn.contains(&id));

        if self.last.unclippable > 0 {
            tracing::warn!(
                items = self.last.unclippable,
                "vector items left undrawn because a residual clip had no shape to apply"
            );
        }
        if self.last.unpaintable > 0 {
            tracing::debug!(
                items = self.last.unpaintable,
                "vector items left undrawn because nothing here paints what they asked for"
            );
        }
        Ok(())
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
