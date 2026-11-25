//! Whole drawings rasterised on the CPU into images, and the rules that decide when.
//!
//! A drawing that would need the general rasteriser for a gradient or a clip is painted whole into
//! one premultiplied sRGB raster and drawn as one image. The image rides the box's transform and
//! clip, so a moved replay and a scroll move it at no cost, and a static page of such drawings never
//! builds the general rasteriser.
//!
//! A raster is exact for one linear map and one sixteenth-pixel phase. While a drawing changes
//! scale, a raster of the same source within half to twice the scale is stretched instead, until
//! the drawing holds still for [`SETTLE_FRAMES`] frames. New rasters are budgeted per frame, and a
//! drawing that rasterises too often is drawn shape by shape for a while.
//!
//! The cache keeps each raster's texels, so the compositor's image of it can be made again.

use std::sync::Arc;
use web_time::Instant;

use crate::VectorId;
use iceblink::{ImageDesc, ImageFormat};
use kurbo::{self, Affine};
use rustc_hash::FxHashMap;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_profile::{Counter, counter};

use crate::content::vectors::Drawing;
use crate::content::vectors::cpu::{LayerJob, VectorPainter, Zeno};
use crate::emit::record::{ImageKey, ImagePixels};
use crate::emit::vector::ShapePaint;

mod tiles;

pub use tiles::{Tile, Tiles};

/// What a drawing asks the layer cache for.
#[derive(Clone, Copy, Debug)]
pub struct LayerRequest<'a> {
    /// The drawing's element, as the emit walk names it.
    ///
    /// The element and not its fragment, because a transform that appears above a drawing
    /// rebuilds the fragment, and the history has to see the same drawing change scale.
    pub owner: VectorId,
    /// The revision of the drawing's source.
    pub revision: u64,
    /// The drawing.
    pub drawing: &'a Drawing,
    /// What an inherited paint resolves to, before any folded opacity.
    pub paint: ShapePaint,
    /// The matrix the box's transform resolves to, or `None` when it leaves the plane.
    pub spatial: Option<zgui_geom::Affine2>,
}

/// Why a drawing is drawn shape by shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerFallback {
    /// Its shapes need no general rasteriser, so a layer saves nothing.
    PerShape,
    /// No layer can draw it: a turn, a mirror, a stroke under a stretch, or a raster too large.
    Ineligible,
    /// The frame spent its budget for new layers.
    Budget,
    /// It rasterised too often lately.
    Demoted,
}

/// The texels of one raster, and the image the compositor keeps them as.
#[derive(Clone, Debug)]
pub struct LayerPixels {
    /// The image.
    pub key: ImageKey,
    /// The width, in texels.
    pub width: u32,
    /// The height, in texels.
    pub height: u32,
    /// The premultiplied texels, or `None` for a raster not rasterised yet.
    pub texels: Option<Arc<[u8]>>,
}

impl LayerPixels {
    /// Returns the texels as the recorder takes them.
    ///
    /// A raster not rasterised yet is transparent at revision zero, and its texels are at
    /// revision one.
    pub fn pixels(&self) -> ImagePixels<'_> {
        ImagePixels {
            desc: ImageDesc {
                width: self.width,
                height: self.height,
                format: ImageFormat::RGBA8_PREMULTIPLIED,
            },
            revision: u64::from(self.texels.is_some()),
            texels: self.texels.as_deref(),
        }
    }
}

impl PartialEq for LayerPixels {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.width == other.width
            && self.height == other.height
            && match (&self.texels, &other.texels) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

/// What the layer cache answers.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerAnswer {
    /// Draw this raster as one image at `local`, in the fragment's own space.
    Sprite {
        /// The raster.
        raster: LayerPixels,
        /// The image's rectangle.
        local: Rect<DevicePx, Device>,
        /// Whether the raster is of another scale or phase, stretched until the drawing settles.
        provisional: bool,
    },
    /// Draw one image per tile, which the end of the walk rasterises where the frame needs it.
    Tiles {
        /// The tiles.
        tiles: Tiles,
        /// Whether the tiles are of another scale, stretched until the drawing settles.
        provisional: bool,
    },
    /// Draw nothing this frame. The drawing is owed a frame, where it would cover `local`.
    Defer {
        /// Where the sprite would be, in the fragment's own space.
        local: Rect<DevicePx, Device>,
    },
    /// Draw the shapes one by one.
    Items(LayerFallback),
}

/// Where whole drawings are rasterised, as the emit walk sees it.
#[doc(hidden)]
pub trait VectorLayerSource {
    /// The layer for `request`, rasterising it when the route rules allow.
    fn layer(&self, request: LayerRequest<'_>) -> LayerAnswer;

    /// Records that `owner`'s shapes took no general route, so a layer saves it nothing.
    fn per_shape_suffices(&self, owner: VectorId);

    /// Whether the tiled source with number `id` still stands, so a record drawing its tiles may
    /// replay.
    fn tiles_alive(&self, _id: u64) -> bool {
        true
    }

    /// Notes that the tile image `key` is shown at `admitted` on the surface this frame, or
    /// nowhere for `None`.
    fn shown(&self, _key: ImageKey, _admitted: Option<Rect<DevicePx, Device>>) {}

    /// Rasterises the tiles shown this frame where the frame needs them, hands each raster to
    /// `write`, and returns the device rectangles of the tiles the frame needed and deferred.
    fn settle(
        &self,
        _viewport: Size<i32, Device>,
        _damage: &zgui_bits::DamageSet,
        _write: &mut dyn FnMut(&LayerPixels),
    ) -> Vec<Rect<i32, Device>> {
        Vec::new()
    }
}

/// The identity of one raster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LayerKey {
    /// The revision of the source.
    revision: u64,
    /// The address of the shared shapes, which the entry holds so it is not reused.
    shapes: usize,
    /// A hash of the inherited paint, or zero when no shape reads it.
    paint: u64,
    /// The linear map from path space to texels, in 1/4096.
    linear: [i32; 4],
    /// The sixteenth-pixel phase of the translation on each axis.
    phase: [u8; 2],
}

impl LayerKey {
    /// Whether `other` draws the same source in the same paint.
    fn same_source(&self, other: &Self) -> bool {
        self.source() == other.source()
    }

    /// The source and paint, without the map.
    fn source(&self) -> (u64, usize, u64) {
        (self.revision, self.shapes, self.paint)
    }

    /// The dequantised linear map.
    fn linear(&self) -> Affine {
        let [a, b, c, d] = self.linear.map(|value| f64::from(value) / QUANTUM);
        Affine::new([a, b, c, d, 0.0, 0.0])
    }
}

/// One raster.
#[derive(Clone, Debug)]
struct Entry {
    /// The raster's texels and image.
    pixels: LayerPixels,
    /// The shapes, held so the address in the key is not reused while the key stands.
    #[expect(dead_code, reason = "held for its address")]
    shapes: Arc<[zgui_svg::Shape]>,
    /// The raster's rectangle, in path space.
    path_bounds: kurbo::Rect,
    /// The raster's bytes.
    bytes: u64,
    /// The frame it was last drawn in.
    used: u32,
    /// The uniform scale of its linear map.
    scale: f64,
}

/// What the cache remembers about one drawing across frames.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LayerHistory {
    /// The key it asked for last.
    key: Option<LayerKey>,
    /// How many frames in a row it asked for that key.
    stable: u32,
    /// The frame it last asked in.
    seen: u32,
    /// Whether its last change kept the source and the paint.
    same: bool,
    /// The frames of its last three rasters, the oldest first. Zero is none.
    rasters: [u32; DEMOTE_RASTERS],
    /// The frame it may take a first raster again from.
    demoted_until: u32,
    /// How many frames in a row it was deferred.
    deferred: u8,
    /// The revision and shapes address of the source it asked for last.
    source: (u64, usize),
    /// Whether the shapes of that source took no general route, so a layer saves it nothing.
    per_shape: bool,
}

/// What a source costs to rasterise, in path space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SourceCost {
    /// The area of every shape's control box, grown by its own stroke.
    pub(crate) area: f64,
    /// How many path elements the shapes have.
    pub(crate) segments: usize,
    /// How many shapes have a stroke of their own.
    pub(crate) strokes: usize,
}

impl SourceCost {
    /// The cost of `shapes`.
    pub(crate) fn of(shapes: &[zgui_svg::Shape]) -> Self {
        let mut cost = Self::default();
        for shape in shapes {
            let reach = shape
                .stroke
                .as_ref()
                .map_or(0.0, |stroke| reach(&stroke.style));
            cost.area += shape.path.control_box().inflate(reach, reach).area();
            cost.segments += shape.path.elements().len();
            cost.strokes += usize::from(shape.stroke.is_some());
        }
        cost
    }

    /// The estimated microseconds of one raster under a map of determinant `det`.
    fn us(&self, det: f64) -> f64 {
        US_PER_PIXEL * self.area * det.abs() + US_PER_SEGMENT * self.segments as f64
    }
}

/// What one frame has spent on new layers.
#[derive(Clone, Copy, Debug, Default)]
struct Spent {
    /// Layers rasterised.
    layers: u32,
    /// Their estimated microseconds.
    us: f64,
    /// The estimated microseconds of first rasters while the general rasteriser is cold.
    cold_us: f64,
}

/// The layers a window keeps, and the history of each drawing that asked for one.
#[derive(Debug, Default)]
pub(crate) struct VectorLayerCache {
    /// The rasters, by key.
    entries: FxHashMap<LayerKey, Entry>,
    /// Each drawing's history, by its fragment's base identity.
    histories: FxHashMap<VectorId, LayerHistory>,
    /// What each source costs, by revision and shapes address.
    costs: FxHashMap<(u64, usize), SourceCost>,
    /// The current frame, advanced by [`VectorLayerCache::begin_frame`]. Zero before the first.
    frame: u32,
    /// Whether the general vector rasteriser is built.
    raster_ready: bool,
    /// The handle the next raster takes, below the namespace.
    next_handle: u64,
    /// The bytes of every entry's raster.
    bytes: u64,
    /// What this frame has spent.
    spent: Spent,
    /// Rasters of scales no drawing uses any more, removed when the frame ends.
    superseded: Vec<LayerKey>,
    /// What paints the rasters.
    painter: Zeno,
    /// How many images the cache has answered, monotonic.
    hits: u64,
    /// The tiled sources of large drawings, and their rasters.
    tiles: tiles::TileCache,
}

/// The image key namespace of whole layer rasters.
///
/// Image node handles reach it only past node index `0x7D00_0000`.
pub(crate) const LAYER_NAMESPACE: u64 = 0x7D00_0000_0000_0000;
const HANDLE_BITS: u64 = 0x00FF_FFFF_FFFF_FFFF;

/// The steps of a pixel the translation is rasterised at.
///
/// A sixteenth puts an edge within a thirty-second of a pixel of where the general rasteriser puts
/// it. A quarter, as glyphs use, leaves an eighth, which a thin stroke shows as a tenth of its
/// coverage along every edge.
pub(crate) const PHASES: u32 = 16;

/// The phase of the device coordinate `t`: its fraction in steps of [`PHASES`].
pub(crate) fn phase_of(t: f64) -> u8 {
    let steps = f64::from(PHASES);
    ((steps * (t - t.floor())).round() as u32 % PHASES) as u8
}

/// The steps of the quantised linear map, per unit.
const QUANTUM: f64 = 4096.0;

/// How much residual turn a matrix may carry and still be read as axis-aligned, relative to its
/// scale.
const EPSILON: f64 = 1.0e-4;

/// How many stable frames a provisional layer waits before it is rasterised exactly.
pub(crate) const SETTLE_FRAMES: u32 = 3;

/// How many stable frames a first raster waits while the general rasteriser is built.
pub(crate) const STABLE_FRAMES: u32 = 2;

/// The estimate under which a first raster does not wait.
pub(crate) const CHEAP_US: f64 = 50.0;

/// How many frames a drawing is deferred before it rasterises over the budget.
pub(crate) const DEFER_FRAMES: u8 = 2;

/// The most new layers one frame rasterises once the first is admitted.
pub(crate) const FRAME_LAYERS: u32 = 3;

/// The estimated microseconds one frame spends on new layers once the first is admitted.
pub(crate) const FRAME_US: f64 = 2_000.0;

/// The estimated microseconds one frame spends on first rasters while the general rasteriser is
/// cold.
pub(crate) const COLD_US: f64 = 8_000.0;

/// The largest estimate one layer may have.
pub(crate) const MAX_US: f64 = 8_000.0;

/// The longest side of one layer, in texels.
pub(crate) const MAX_SIDE: u32 = 2048;

/// The most bytes one layer may have, its levels of detail aside.
pub(crate) const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// How many rasters within [`DEMOTE_WINDOW`] frames demote a drawing.
const DEMOTE_RASTERS: usize = 3;

/// The frames the demotion counts over, and how long it lasts.
const DEMOTE_WINDOW: u32 = 30;

/// How many frames a history survives without a request.
const HISTORY_FRAMES: u32 = 64;

/// How many scales of one source are kept.
const KEPT_SCALES: usize = 2;

/// Estimated microseconds per texel, measured by `calibrate_the_cost_model`.
const US_PER_PIXEL: f64 = 0.004;

/// Estimated microseconds per path element, measured by `calibrate_the_cost_model`.
const US_PER_SEGMENT: f64 = 2.0;

/// One request worked out: its key, its raster and where the sprite goes.
#[derive(Clone, Copy, Debug)]
struct Geometry {
    key: LayerKey,
    /// Path space to texels.
    map: Affine,
    width: u32,
    height: u32,
    /// The raster's rectangle in path space.
    path_bounds: kurbo::Rect,
    /// The uniform scale of the dequantised map.
    scale: f64,
    /// What a shape's own stroke is multiplied by.
    stroke_scale: f64,
    /// The element's stroke in texels.
    inherited_stroke: f64,
    /// The determinant of the dequantised map.
    det: f64,
}

/// How a route decided.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Route {
    /// The exact raster exists.
    Hit,
    /// A raster of another scale stands in.
    Provisional(LayerKey),
    /// Rasterise now.
    Raster,
    /// Draw nothing this frame.
    Defer,
    /// Draw the shapes.
    Items(LayerFallback),
}

impl VectorLayerCache {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1).max(1);
        self.spent = Spent::default();
        counter::set(Counter::VectorLayerBytesLive, self.bytes());
    }

    /// Sets whether the general vector rasteriser is built.
    pub(crate) fn set_raster_ready(&mut self, ready: bool) {
        self.raster_ready = ready;
    }

    /// Ends a frame.
    ///
    /// Removes the superseded rasters this frame did not draw, then forgets the drawings no
    /// request touched for [`HISTORY_FRAMES`] frames and the costs nothing names.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        for key in core::mem::take(&mut self.superseded) {
            if self
                .entries
                .get(&key)
                .is_some_and(|entry| entry.used != frame)
            {
                self.remove(&key);
            }
        }
        self.sweep_tiles();
        self.histories
            .retain(|_, history| frame.wrapping_sub(history.seen) < HISTORY_FRAMES);
        if self.costs.len() > self.entries.len() + self.histories.len() {
            let mut named: rustc_hash::FxHashSet<(u64, usize)> = self
                .entries
                .keys()
                .map(|key| (key.revision, key.shapes))
                .collect();
            named.extend(
                self.histories
                    .values()
                    .filter_map(|history| history.key)
                    .map(|key| (key.revision, key.shapes)),
            );
            self.costs.retain(|source, _| named.contains(source));
        }
    }

    /// The bytes of every raster, the tiles of large drawings included.
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes + self.tiles.bytes()
    }

    /// The bytes of the rasters this frame drew.
    pub(crate) fn pinned_bytes(&self) -> u64 {
        self.entries
            .values()
            .filter(|entry| entry.used == self.frame)
            .map(|entry| entry.bytes)
            .sum::<u64>()
            + self.pinned_tile_bytes()
    }

    /// How many rasters the cache keeps, the tiles of large drawings included.
    pub(crate) fn len(&self) -> usize {
        self.entries.len() + self.tiles.rasterised()
    }

    /// How many images the cache has answered, monotonic.
    pub(crate) fn hits(&self) -> u64 {
        self.hits
    }

    /// Removes the least recently drawn rasters this frame did not draw until `bytes` have gone,
    /// and reports how many went.
    pub(crate) fn evict(&mut self, bytes: u64) -> u64 {
        /// One raster the budget may remove.
        #[derive(Clone, Copy)]
        enum Cold {
            Layer(LayerKey),
            Tile(u64),
        }
        let frame = self.frame;
        let mut cold: Vec<(u32, Cold)> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.used != frame)
            .map(|(key, entry)| (entry.used, Cold::Layer(*key)))
            .collect();
        cold.extend(
            self.cold_tiles()
                .into_iter()
                .map(|(used, handle)| (used, Cold::Tile(handle))),
        );
        cold.sort_unstable_by_key(|(used, _)| *used);
        let mut freed = 0;
        for (_, which) in cold {
            if freed >= bytes {
                break;
            }
            let gone = match which {
                Cold::Layer(key) => self.remove(&key),
                Cold::Tile(handle) => self.evict_tile(handle),
            };
            if let Some(gone) = gone {
                freed += gone;
                counter::bump(Counter::VectorLayersEvicted);
            }
        }
        freed
    }

    /// Forgets everything.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.histories.clear();
        self.costs.clear();
        self.superseded.clear();
        self.bytes = 0;
        self.tiles.clear();
    }

    /// Removes the raster of `key`, and returns its bytes.
    fn remove(&mut self, key: &LayerKey) -> Option<u64> {
        let entry = self.entries.remove(key)?;
        self.bytes = self.bytes.saturating_sub(entry.bytes);
        Some(entry.bytes)
    }

    /// Records that the shapes of `owner`'s last source took no general route.
    pub(crate) fn per_shape_suffices(&mut self, owner: VectorId) {
        let frame = self.frame;
        let history = self.histories.entry(owner).or_default();
        history.per_shape = true;
        history.seen = frame;
    }

    /// The layer for `request`.
    pub(crate) fn layer(&mut self, request: LayerRequest<'_>) -> LayerAnswer {
        let drawing = request.drawing;
        let source = (request.revision, Arc::as_ptr(&drawing.shapes).addr());
        let history = self.histories.entry(request.owner).or_default();
        // A new source can need the general route, so it asks again.
        if history.source != source {
            history.source = source;
            history.per_shape = false;
        }
        if history.per_shape {
            return LayerAnswer::Items(LayerFallback::PerShape);
        }
        let cost = *self
            .costs
            .entry(source)
            .or_insert_with(|| SourceCost::of(&drawing.shapes));
        let mapping = mapping(&request, &cost);
        if let Some(mapping) = mapping.filter(Mapping::too_large) {
            return self.tiled(&request, &mapping);
        }
        let geometry = mapping.as_ref().and_then(whole);
        let route = self.route(request.owner, geometry.as_ref(), &cost);
        let Some(geometry) = geometry else {
            return LayerAnswer::Items(LayerFallback::Ineligible);
        };
        let local = |path_bounds: kurbo::Rect| rect(drawing.fit.transform_rect_bbox(path_bounds));
        if matches!(route, Route::Hit | Route::Provisional(_) | Route::Raster) {
            self.hits += 1;
        }
        match route {
            Route::Hit => {
                let entry = &self.entries[&geometry.key];
                LayerAnswer::Sprite {
                    raster: entry.pixels.clone(),
                    local: local(geometry.path_bounds),
                    provisional: false,
                }
            }
            Route::Provisional(near) => {
                let entry = self
                    .entries
                    .get_mut(&near)
                    .expect("a near key has an entry");
                entry.used = self.frame;
                LayerAnswer::Sprite {
                    raster: entry.pixels.clone(),
                    local: local(entry.path_bounds),
                    provisional: true,
                }
            }
            Route::Defer => LayerAnswer::Defer {
                local: local(geometry.path_bounds),
            },
            Route::Items(why) => LayerAnswer::Items(why),
            Route::Raster => LayerAnswer::Sprite {
                raster: self.raster(&request, &geometry, &cost),
                local: local(geometry.path_bounds),
                provisional: false,
            },
        }
    }

    /// Decides how `owner` draws this frame, and updates its history.
    fn route(&mut self, owner: VectorId, geometry: Option<&Geometry>, cost: &SourceCost) -> Route {
        let frame = self.frame;
        let history = self.note_key(owner, geometry.map(|geometry| geometry.key));
        let Some(geometry) = geometry else {
            return Route::Items(LayerFallback::Ineligible);
        };
        let us = cost.us(geometry.det);
        if us > MAX_US {
            return Route::Items(LayerFallback::Ineligible);
        }
        if let Some(entry) = self.entries.get_mut(&geometry.key) {
            entry.used = frame;
            return Route::Hit;
        }
        let near = if history.same {
            self.nearest(&geometry.key, geometry.scale)
        } else {
            None
        };
        if let Some(near) = near {
            if history.stable < SETTLE_FRAMES {
                return Route::Provisional(near);
            }
            // An upgrade, cold or warm, takes the frame's budget.
            return if self.admits(us) {
                Route::Raster
            } else {
                Route::Provisional(near)
            };
        }
        let history = self.histories.get_mut(&owner).expect("just inserted");
        if !self.raster_ready {
            if self.spent.cold_us + us <= COLD_US || history.deferred >= DEFER_FRAMES {
                return Route::Raster;
            }
            history.deferred = history.deferred.saturating_add(1);
            return Route::Defer;
        }
        if frame < history.demoted_until {
            return Route::Items(LayerFallback::Demoted);
        }
        let waited = us < CHEAP_US || history.stable >= STABLE_FRAMES;
        if waited && self.admits(us) {
            Route::Raster
        } else {
            Route::Items(LayerFallback::Budget)
        }
    }

    /// Records that `owner` asked for `key` this frame, and returns its history.
    ///
    /// Once per frame, so a drawing encoded twice in one frame does not count twice.
    fn note_key(&mut self, owner: VectorId, key: Option<LayerKey>) -> LayerHistory {
        let frame = self.frame;
        let history = self.histories.entry(owner).or_default();
        if history.seen != frame {
            if history.key.is_some() && history.key == key {
                history.stable = history.stable.saturating_add(1);
            } else {
                history.same = match (history.key, key) {
                    (Some(old), Some(new)) => old.same_source(&new),
                    _ => false,
                };
                history.key = key;
                history.stable = 0;
            }
            history.seen = frame;
        }
        *history
    }

    /// Records that `owner` made a new raster this frame, and demotes it when that is the third in
    /// [`DEMOTE_WINDOW`] frames.
    fn note_raster(&mut self, owner: VectorId) {
        let frame = self.frame;
        if let Some(history) = self.histories.get_mut(&owner) {
            history.deferred = 0;
            history.rasters.rotate_left(1);
            history.rasters[DEMOTE_RASTERS - 1] = frame;
            let oldest = history.rasters[0];
            if oldest != 0 && frame.wrapping_sub(oldest) < DEMOTE_WINDOW {
                history.demoted_until = frame.wrapping_add(DEMOTE_WINDOW);
                counter::bump(Counter::VectorLayersDemoted);
            }
        }
    }

    /// Whether the frame's budget admits one more layer of `us`.
    ///
    /// The first layer of a frame is always admitted, so a layer that alone costs more than the
    /// budget still settles.
    fn admits(&self, us: f64) -> bool {
        self.spent.layers == 0
            || (self.spent.layers < FRAME_LAYERS && self.spent.us + us <= FRAME_US)
    }

    /// The entry of the same source and paint whose scale is closest to `scale`, within half to
    /// twice it.
    fn nearest(&self, key: &LayerKey, scale: f64) -> Option<LayerKey> {
        self.entries
            .iter()
            .filter(|(held, _)| held.same_source(key))
            .filter(|(_, entry)| (0.5..=2.0).contains(&(entry.scale / scale)))
            .min_by(|(_, a), (_, b)| {
                let distance = |entry: &Entry| (entry.scale / scale).ln().abs();
                distance(a).total_cmp(&distance(b))
            })
            .map(|(held, _)| *held)
    }

    /// Rasterises `geometry` into a new raster, records it and returns it.
    fn raster(
        &mut self,
        request: &LayerRequest<'_>,
        geometry: &Geometry,
        cost: &SourceCost,
    ) -> LayerPixels {
        let started = Instant::now();
        let mut texels = vec![0; geometry.width as usize * geometry.height as usize * 4];
        self.painter.paint(
            &LayerJob {
                shapes: &request.drawing.shapes,
                only: None,
                paint: request.paint,
                map: geometry.map,
                stroke_scale: geometry.stroke_scale,
                inherited_stroke: geometry.inherited_stroke,
                width: geometry.width,
                height: geometry.height,
            },
            &mut texels,
        );
        let bytes = texels.len() as u64;
        let pixels = LayerPixels {
            key: ImageKey(LAYER_NAMESPACE | self.next_handle),
            width: geometry.width,
            height: geometry.height,
            texels: Some(Arc::from(texels)),
        };
        self.next_handle = (self.next_handle + 1) & HANDLE_BITS;
        counter::bump(Counter::VectorLayersRasterised);
        counter::add(
            Counter::VectorLayerRasterUs,
            started.elapsed().as_micros() as u64,
        );
        counter::add(Counter::VectorLayerBytesUploaded, bytes);

        let frame = self.frame;
        let us = cost.us(geometry.det);
        self.spent.layers += 1;
        self.spent.us += us;
        if !self.raster_ready {
            self.spent.cold_us += us;
        }
        self.note_raster(request.owner);
        self.entries.insert(
            geometry.key,
            Entry {
                pixels: pixels.clone(),
                shapes: Arc::clone(&request.drawing.shapes),
                path_bounds: geometry.path_bounds,
                bytes,
                used: frame,
                scale: geometry.scale,
            },
        );
        self.bytes += bytes;
        self.supersede(&geometry.key);
        pixels
    }

    /// Sends the rasters of `key`'s source beyond its [`KEPT_SCALES`] most recently drawn scales
    /// to be removed when the frame ends.
    fn supersede(&mut self, key: &LayerKey) {
        let mut scales: Vec<([i32; 4], u32)> = Vec::new();
        for (held, entry) in &self.entries {
            if !held.same_source(key) {
                continue;
            }
            match scales.iter_mut().find(|(linear, _)| *linear == held.linear) {
                Some((_, used)) => *used = (*used).max(entry.used),
                None => scales.push((held.linear, entry.used)),
            }
        }
        if scales.len() <= KEPT_SCALES {
            return;
        }
        scales.sort_unstable_by_key(|(_, used)| core::cmp::Reverse(*used));
        let kept: Vec<[i32; 4]> = scales
            .iter()
            .take(KEPT_SCALES)
            .map(|(linear, _)| *linear)
            .collect();
        for held in self.entries.keys() {
            if held.same_source(key) && !kept.contains(&held.linear) {
                self.superseded.push(*held);
            }
        }
    }
}

/// The map of one request, before the raster is cut to a rectangle.
#[derive(Clone, Copy, Debug)]
struct Mapping {
    key: LayerKey,
    /// Path space to texels, the phase included.
    placed: Affine,
    /// The dequantised linear map.
    linear: Affine,
    /// The texels every shape inks, under `placed`.
    bounds: kurbo::Rect,
    /// The uniform scale of the dequantised map.
    scale: f64,
    /// What a shape's own stroke is multiplied by.
    stroke_scale: f64,
    /// The element's stroke in texels.
    inherited_stroke: f64,
    /// The determinant of the dequantised map.
    det: f64,
}

impl Mapping {
    /// Whether one raster of these bounds is past the side or the byte cap.
    fn too_large(&self) -> bool {
        let width = self.bounds.x1.ceil() - self.bounds.x0.floor();
        let height = self.bounds.y1.ceil() - self.bounds.y0.floor();
        width > f64::from(MAX_SIDE)
            || height > f64::from(MAX_SIDE)
            || width * height * 4.0 > MAX_BYTES as f64
    }
}

/// The raster of `mapping` as one tile, or `None` when it is empty or too large.
fn whole(mapping: &Mapping) -> Option<Geometry> {
    let bounds = mapping.bounds;
    let origin = kurbo::Point::new(bounds.x0.floor(), bounds.y0.floor());
    let width = bounds.x1.ceil() - origin.x;
    let height = bounds.y1.ceil() - origin.y;
    if !(width >= 1.0 && height >= 1.0) || mapping.too_large() {
        return None;
    }
    let raster = kurbo::Rect::new(origin.x, origin.y, origin.x + width, origin.y + height);
    let shift = mapping.placed.translation();
    let path_bounds = mapping.linear.inverse().transform_rect_bbox(raster - shift);
    Some(Geometry {
        key: mapping.key,
        map: Affine::translate(-origin.to_vec2()) * mapping.placed,
        width: width as u32,
        height: height as u32,
        path_bounds,
        scale: mapping.scale,
        stroke_scale: mapping.stroke_scale,
        inherited_stroke: mapping.inherited_stroke,
        det: mapping.det,
    })
}

/// Works out the key and the map of `request`, or `None` when no layer can draw it.
fn mapping(request: &LayerRequest<'_>, cost: &SourceCost) -> Option<Mapping> {
    let spatial = request.spatial?;
    let (a, d) = (f64::from(spatial.a), f64::from(spatial.d));
    let (b, c) = (f64::from(spatial.b), f64::from(spatial.c));
    // A mirror or a turn would turn the sprite a second time.
    if !(a > 0.0 && d > 0.0) || b.abs() > EPSILON * a.max(d) || c.abs() > EPSILON * a.max(d) {
        return None;
    }
    let fit = request.drawing.fit;
    let fit_det = fit.determinant();
    if !fit_det.is_finite() || fit_det.abs() <= 1.0e-12 {
        return None;
    }
    let [fa, fb, fc, fd, fe, ff] = fit.as_coeffs();
    // The raster's rectangle stays a rectangle in the fragment's space only for an upright fit.
    let fit_scale = fa.abs().max(fd.abs());
    if fb.abs() > EPSILON * fit_scale || fc.abs() > EPSILON * fit_scale {
        return None;
    }
    let shapes = &request.drawing.shapes;
    let paint = request.paint;
    let inherited_strokes =
        paint.stroke.is_some() && shapes.iter().any(|shape| shape.stroke.is_none());
    let strokes = cost.strokes > 0 || inherited_strokes;
    let linear = [a * fa, d * fb, a * fc, d * fd];
    if strokes {
        let (x, y) = (linear[0].abs(), linear[3].abs());
        if (x - y).abs() > EPSILON * x.max(y) {
            return None;
        }
    }
    let mut quantised = [0_i32; 4];
    for (out, value) in quantised.iter_mut().zip(linear) {
        let steps = (value * QUANTUM).round();
        if !steps.is_finite() || steps.abs() > f64::from(i32::MAX) {
            return None;
        }
        *out = steps as i32;
    }
    let translation = [
        a * fe + f64::from(spatial.tx),
        d * ff + f64::from(spatial.ty),
    ];
    let phase = translation.map(phase_of);
    let reads_paint = inherited_strokes || shapes.iter().any(zgui_svg::Shape::is_inherited);
    let key = LayerKey {
        revision: request.revision,
        shapes: Arc::as_ptr(shapes).addr(),
        paint: if reads_paint { paint_hash(paint) } else { 0 },
        linear: quantised,
        phase,
    };
    let dequantised = key.linear();
    let det = dequantised.determinant();
    if !det.is_finite() || det.abs() <= 1.0e-12 {
        return None;
    }
    let scale = det.abs().sqrt();
    let stroke_scale = if strokes { scale } else { 1.0 };
    let inherited_stroke = f64::from(paint.stroke_width)
        / zgui_svg::document::place::uniform_scale(fit)
        * stroke_scale;
    let shift = kurbo::Vec2::new(
        f64::from(phase[0]) / f64::from(PHASES),
        f64::from(phase[1]) / f64::from(PHASES),
    );
    let placed = Affine::translate(shift) * dequantised;

    // The union of every shape's ink, each cut to its clips.
    let mut bounds: Option<kurbo::Rect> = None;
    for shape in shapes.iter() {
        let reach = match &shape.stroke {
            Some(stroke) => reach(&stroke.style) * stroke_scale,
            None if paint.stroke.is_some() => reach(&kurbo::Stroke::new(inherited_stroke)),
            None => 0.0,
        };
        if shape.fill.is_none() && reach == 0.0 {
            continue;
        }
        let mut ink = placed
            .transform_rect_bbox(shape.path.control_box())
            .inflate(reach, reach);
        for clip in &shape.clips {
            ink = ink.intersect(placed.transform_rect_bbox(clip.path.control_box()));
        }
        if ink.width() <= 0.0 || ink.height() <= 0.0 {
            continue;
        }
        bounds = Some(bounds.map_or(ink, |held| held.union(ink)));
    }
    Some(Mapping {
        key,
        placed,
        linear: dequantised,
        bounds: bounds?,
        scale,
        stroke_scale,
        inherited_stroke,
        det,
    })
}

/// A hash of what an inherited paint resolves to.
fn paint_hash(paint: ShapePaint) -> u64 {
    let mut hash = zgui_space::ContentHash::new()
        .f32s(&paint.fill.to_premultiplied_srgb())
        .f32(paint.stroke_width);
    if let Some(stroke) = paint.stroke {
        hash = hash.u32(1).f32s(&stroke.to_premultiplied_srgb());
    }
    hash.finish().max(1)
}

/// How far a stroke's outline reaches past the path, at most.
fn reach(stroke: &kurbo::Stroke) -> f64 {
    let half = stroke.width.max(0.0) / 2.0;
    let miter = match stroke.join {
        kurbo::Join::Miter => stroke.miter_limit.max(1.0),
        kurbo::Join::Round | kurbo::Join::Bevel => 1.0,
    };
    let squared = |cap| cap == kurbo::Cap::Square;
    let cap = if squared(stroke.start_cap) || squared(stroke.end_cap) {
        core::f64::consts::SQRT_2
    } else {
        1.0
    };
    half * miter.max(cap)
}

/// A rectangle in the fragment's space.
fn rect(bounds: kurbo::Rect) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(bounds.x0 as f32), DevicePx(bounds.y0 as f32)),
        Size::new(
            DevicePx(bounds.width() as f32),
            DevicePx(bounds.height() as f32),
        ),
    )
}

#[cfg(test)]
mod tests;
