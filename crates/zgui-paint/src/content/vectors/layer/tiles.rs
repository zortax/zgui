//! Large drawings in tiles of images, rasterised only where a frame shows them.
//!
//! A drawing whose raster is past the side or the byte cap of one layer is cut into tiles of
//! [`TILE`] texels in the raster space of its linear map. The origin of that space is the origin of
//! the path, so a translation moves every image and keeps every tile. Each tile is one image, which
//! is transparent until its raster is written. The emit walk records the images and notes where the
//! frame shows them. [`VectorLayerCache::settle`] runs after the walk: it rasterises a tile only
//! when the frame needs it, within the frame's layer budget, and writes the raster into its image.
//!
//! A tile's raster is keyed by what it shows: the linear map, its cell and texels, and the shapes
//! that meet the cell, by path address and paint. An edit of one shape of a canvas makes a new
//! source, and every tile that the shape does not meet finds its raster again.

use std::sync::Arc;
use web_time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};
use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_profile::{Counter, counter};
use zgui_scene::ContentHash;
use zgui_scene::kurbo::{self, Affine};

use super::{
    COLD_US, DEFER_FRAMES, HISTORY_FRAMES, LayerAnswer, LayerFallback, LayerKey, LayerPixels,
    LayerRequest, Mapping, SETTLE_FRAMES, VectorLayerCache, paint_hash, reach,
};
use crate::content::vectors::cpu::{LayerJob, VectorPainter};
use crate::emit::record::ImageKey;
use crate::emit::vector::ShapePaint;

/// The side of one tile, in texels.
pub(crate) const TILE: u32 = 512;

/// The image key namespace of tile rasters.
pub(crate) const TILE_NAMESPACE: u64 = 0x7E00_0000_0000_0000;

/// The bits of a handle that name its namespace.
const NAMESPACE_BITS: u64 = 0xFF00_0000_0000_0000;

/// The bits of a handle below its namespace.
const HANDLE_BITS: u64 = 0x00FF_FFFF_FFFF_FFFF;

/// The estimated microseconds one frame spends on tiles and layers together, once its first is
/// admitted.
///
/// The cold cap of whole layers, warm or cold: a tile is rasterised only where the frame shows it,
/// so what a frame spends is bounded by the surface.
pub(crate) const TILE_FRAME_US: f64 = COLD_US;

/// Estimated microseconds per texel a shape inks in a tile, measured by
/// `calibrate_the_tile_cost_model`.
const TILE_US_PER_PIXEL: f64 = 0.0007;

/// Estimated microseconds per path element of a shape in a tile, measured by
/// `calibrate_the_tile_cost_model`.
const TILE_US_PER_SEGMENT: f64 = 0.3;

/// Estimated microseconds per shape in a tile, measured by `calibrate_the_tile_cost_model`.
const TILE_US_PER_SHAPE: f64 = 12.0;

/// What one tile costs to rasterise.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TileCost {
    /// The texels each shape inks in the tile, summed over the shapes.
    pixels: f64,
    /// How many path elements the shapes have.
    segments: usize,
    /// How many shapes meet the tile.
    shapes: usize,
}

impl TileCost {
    /// The estimated microseconds.
    ///
    /// A tile is one cell of a large drawing, so most of its shapes are large and a part of each
    /// is in it. Their cost per texel is lower than the cost per texel of a whole small drawing,
    /// and each shape has a fixed cost.
    fn us(&self) -> f64 {
        TILE_US_PER_PIXEL * self.pixels
            + TILE_US_PER_SEGMENT * self.segments as f64
            + TILE_US_PER_SHAPE * self.shapes as f64
    }
}

/// The most tiles one source may have.
pub(crate) const MAX_TILES: usize = 4096;

/// The identity of a tiled source: a whole-layer key without its phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SourceKey {
    /// The revision of the source.
    revision: u64,
    /// The address of the shared shapes.
    shapes: usize,
    /// A hash of the inherited paint, or zero when no shape reads it.
    paint: u64,
    /// The linear map from path space to texels, in 1/4096.
    linear: [i32; 4],
}

impl SourceKey {
    fn of(key: &LayerKey) -> Self {
        Self {
            revision: key.revision,
            shapes: key.shapes,
            paint: key.paint,
            linear: key.linear,
        }
    }

    /// Whether `other` draws the same source in the same paint.
    fn same_source(&self, other: &Self) -> bool {
        (self.revision, self.shapes, self.paint) == (other.revision, other.shapes, other.paint)
    }
}

/// What a tile's raster shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TileKey {
    /// The linear map, in 1/4096.
    linear: [i32; 4],
    /// The cell's column.
    column: i32,
    /// The cell's row.
    row: i32,
    /// The texels of the cell the raster covers, as `[x0, y0, x1, y1]`.
    ///
    /// The drawing's bounds cut an edge cell, and an edit elsewhere can move them.
    texels: [i32; 4],
    /// A hash of the shapes that meet the cell, in painting order, and the paint they read.
    deps: u64,
}

/// One tile of a source.
#[derive(Debug)]
pub(crate) struct Slot {
    /// The handle of its raster.
    handle: u64,
    /// Its column and row in the grid.
    cell: [i32; 2],
    /// Its texels in the raster space of the linear map, as `[x0, y0, x1, y1]`.
    texels: [i32; 4],
    /// The same rectangle in path space.
    path_rect: kurbo::Rect,
    /// The indices of the shapes that meet it, in painting order.
    shapes: Box<[u32]>,
    /// What it costs to rasterise.
    cost: TileCost,
}

/// One drawing at one linear map, cut into tiles.
#[derive(Debug)]
pub(crate) struct TiledSource {
    /// A number no other source of the cache had.
    id: u64,
    /// What the cache keeps it under.
    key: SourceKey,
    /// The shapes, held so their addresses are not reused while a raster stands.
    shapes: Arc<[zgui_svg::Shape]>,
    /// What an inherited paint resolves to.
    paint: ShapePaint,
    /// The dequantised linear map, path space to texels.
    linear: Affine,
    /// The uniform scale of `linear`.
    scale: f64,
    /// What a shape's own stroke is multiplied by.
    stroke_scale: f64,
    /// The element's stroke in texels.
    inherited_stroke: f64,
    /// The tiles that some shape meets.
    slots: Box<[Slot]>,
}

/// The tiles of one drawing, as the layer cache answers them.
#[derive(Clone, Debug, PartialEq)]
pub struct Tiles {
    /// The number of the source.
    id: u64,
    /// Every tile.
    tiles: Arc<[Tile]>,
}

/// One tile of a drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    /// The tile's raster.
    pub pixels: LayerPixels,
    /// The tile's rectangle in path space.
    pub path_rect: kurbo::Rect,
}

impl Tiles {
    /// The number of the source, which a paint record keeps to know it still stands.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// How many tiles the drawing has.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Whether the drawing has no tile.
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Every tile.
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }
}

/// One tile raster.
#[derive(Debug)]
struct Raster {
    /// What it shows.
    key: TileKey,
    /// The source it was made for, which says how to paint it.
    source: Arc<TiledSource>,
    /// Its slot in that source.
    slot: u32,
    /// Its texels, once rasterised.
    texels: Option<Arc<[u8]>>,
    /// Its bytes while rasterised.
    bytes: u64,
    /// The frame it was last drawn in.
    used: u32,
    /// How many frames in a row it was needed and deferred, up to `deferred_at`.
    deferred: u8,
    /// The frame it was last deferred in.
    deferred_at: u32,
    /// How many sources name it.
    owners: u32,
}

impl Raster {
    /// How many frames in a row before `frame` it was needed and deferred.
    fn streak(&self, frame: u32) -> u8 {
        match frame.wrapping_sub(self.deferred_at) {
            0 => self.deferred.saturating_sub(1),
            1 => self.deferred,
            _ => 0,
        }
    }
}

/// One source the cache keeps.
#[derive(Debug)]
struct SourceEntry {
    source: Arc<TiledSource>,
    /// The frame a drawing last asked for it.
    used: u32,
}

/// The tiled sources of a window and their rasters.
#[derive(Debug)]
pub(crate) struct TileCache {
    sources: FxHashMap<SourceKey, SourceEntry>,
    /// The numbers of the sources in `sources`.
    ids: FxHashSet<u64>,
    /// The rasters, by handle.
    rasters: FxHashMap<u64, Raster>,
    /// The handle of each raster, by what it shows.
    by_key: FxHashMap<TileKey, u64>,
    /// The handle the next raster takes, below the namespace.
    next_handle: u64,
    /// The number the next source takes.
    next_id: u64,
    /// The bytes of every rasterised tile.
    bytes: u64,
    /// Where this frame shows each tile on the surface, by handle, in the order it was noted.
    shown: Vec<(u64, Option<Rect<DevicePx, Device>>)>,
}

impl Default for TileCache {
    fn default() -> Self {
        Self {
            sources: FxHashMap::default(),
            ids: FxHashSet::default(),
            rasters: FxHashMap::default(),
            by_key: FxHashMap::default(),
            next_handle: 0,
            next_id: 1,
            bytes: 0,
            shown: Vec::new(),
        }
    }
}

/// The image key of the raster with `handle`.
fn tile_key(handle: u64) -> ImageKey {
    ImageKey(handle)
}

/// Returns the handle of the tile raster `key` names, or `None` for another image.
fn handle_of(key: ImageKey) -> Option<u64> {
    (key.0 & NAMESPACE_BITS == TILE_NAMESPACE).then_some(key.0)
}

impl TileCache {
    /// Forgets every source and raster, and keeps handing out handles and numbers nothing had.
    ///
    /// The compositor may still hold images under the handles handed out before.
    pub(crate) fn clear(&mut self) {
        *self = Self {
            next_handle: self.next_handle,
            next_id: self.next_id,
            ..Self::default()
        };
    }

    /// The bytes of every rasterised tile.
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// How many tiles are rasterised.
    pub(crate) fn rasterised(&self) -> usize {
        self.rasters
            .values()
            .filter(|raster| raster.texels.is_some())
            .count()
    }

    /// Every rasterised tile's handle, its bytes and the frame it was last drawn in.
    fn held(&self) -> impl Iterator<Item = (u64, u64, u32)> + '_ {
        self.rasters
            .iter()
            .filter(|(_, raster)| raster.texels.is_some())
            .map(|(handle, raster)| (*handle, raster.bytes, raster.used))
    }

    /// Drops the texels of the raster with `handle`, and returns their bytes.
    fn lost(&mut self, handle: u64) -> Option<u64> {
        let raster = self.rasters.get_mut(&handle)?;
        raster.texels.take()?;
        let bytes = core::mem::take(&mut raster.bytes);
        self.bytes = self.bytes.saturating_sub(bytes);
        Some(bytes)
    }

    /// Whether a source with number `id` stands.
    pub(crate) fn alive(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }

    /// The tiles of `source`, each with the texels of its raster where it has them.
    fn tiles_of(&self, source: &TiledSource) -> Tiles {
        let tiles = source
            .slots
            .iter()
            .map(|slot| {
                let [x0, y0, x1, y1] = slot.texels;
                Tile {
                    pixels: LayerPixels {
                        key: tile_key(slot.handle),
                        width: (x1 - x0) as u32,
                        height: (y1 - y0) as u32,
                        texels: self
                            .rasters
                            .get(&slot.handle)
                            .and_then(|raster| raster.texels.clone()),
                    },
                    path_rect: slot.path_rect,
                }
            })
            .collect();
        Tiles {
            id: source.id,
            tiles,
        }
    }

    /// The source of the same drawing and paint whose scale is closest to `scale`, within half to
    /// twice it.
    fn nearest(&self, key: &SourceKey, scale: f64) -> Option<Arc<TiledSource>> {
        self.sources
            .values()
            .filter(|entry| entry.source.key.same_source(key))
            .filter(|entry| (0.5..=2.0).contains(&(entry.source.scale / scale)))
            .min_by(|a, b| {
                let distance = |entry: &SourceEntry| (entry.source.scale / scale).ln().abs();
                distance(a).total_cmp(&distance(b))
            })
            .map(|entry| Arc::clone(&entry.source))
    }

    /// Cuts the drawing of `request` under `mapping` into tiles, and keeps the source.
    ///
    /// A tile whose shapes and paint equal a tile of a source already kept takes that tile's
    /// raster. A new raster in a cell where `previous`, the drawing's last source, had a raster
    /// that is not rasterised takes over its deferrals, so a drawing that changes on every frame
    /// still draws each cell by its third frame. `None` when the drawing inks nothing or has more
    /// than [`MAX_TILES`] tiles.
    fn build(
        &mut self,
        request: &LayerRequest<'_>,
        mapping: &Mapping,
        key: SourceKey,
        previous: Option<&TiledSource>,
        frame: u32,
    ) -> Option<Arc<TiledSource>> {
        let linear = mapping.linear;
        let shapes = &request.drawing.shapes;
        let paint = request.paint;
        // The ink of every shape in the raster space, cut to its clips.
        let mut boxes: Vec<Option<kurbo::Rect>> = Vec::with_capacity(shapes.len());
        let mut union: Option<kurbo::Rect> = None;
        for shape in shapes.iter() {
            let reach = match &shape.stroke {
                Some(stroke) => reach(&stroke.style) * mapping.stroke_scale,
                None if paint.stroke.is_some() => {
                    reach(&kurbo::Stroke::new(mapping.inherited_stroke))
                }
                None => 0.0,
            };
            if shape.fill.is_none() && reach == 0.0 {
                boxes.push(None);
                continue;
            }
            let mut ink = linear
                .transform_rect_bbox(shape.path.control_box())
                .inflate(reach, reach);
            for clip in &shape.clips {
                ink = ink.intersect(linear.transform_rect_bbox(clip.path.control_box()));
            }
            if !(ink.width() > 0.0 && ink.height() > 0.0) {
                boxes.push(None);
                continue;
            }
            union = Some(union.map_or(ink, |held| held.union(ink)));
            boxes.push(Some(ink));
        }
        let union = union?;
        let side = f64::from(TILE);
        let cell = |value: f64| (value / side).floor() as i64;
        let (c0, r0) = (cell(union.x0), cell(union.y0));
        let (c1, r1) = (cell(union.x1 - 1e-9) + 1, cell(union.y1 - 1e-9) + 1);
        let (columns, rows) = (c1 - c0, r1 - r0);
        if columns <= 0 || rows <= 0 || columns * rows > MAX_TILES as i64 {
            return None;
        }
        let (columns, rows) = (columns as usize, rows as usize);
        let mut cells: Vec<Vec<u32>> = vec![Vec::new(); columns * rows];
        for (index, ink) in boxes.iter().enumerate() {
            let Some(ink) = ink else {
                continue;
            };
            let (x0, y0) = (cell(ink.x0) - c0, cell(ink.y0) - r0);
            let (x1, y1) = (cell(ink.x1 - 1e-9) - c0 + 1, cell(ink.y1 - 1e-9) - r0 + 1);
            for row in y0.max(0)..y1.min(rows as i64) {
                for column in x0.max(0)..x1.min(columns as i64) {
                    cells[row as usize * columns + column as usize].push(index as u32);
                }
            }
        }
        let inherited = paint_hash(paint);
        let reads_paint = |shape: &zgui_svg::Shape| {
            shape.is_inherited() || (shape.stroke.is_none() && paint.stroke.is_some())
        };
        let hashes: Vec<u64> = shapes
            .iter()
            .zip(&boxes)
            .map(|(shape, ink)| ink.map_or(0, |_| shape_hash(shape)))
            .collect();
        let bounds = [
            union.x0.floor() as i64,
            union.y0.floor() as i64,
            union.x1.ceil() as i64,
            union.y1.ceil() as i64,
        ];
        let inverse = linear.inverse();
        let mut slots = Vec::new();
        let mut fresh = Vec::new();
        for row in 0..rows {
            for column in 0..columns {
                let indices = &cells[row * columns + column];
                if indices.is_empty() {
                    continue;
                }
                let (cx, cy) = (c0 + column as i64, r0 + row as i64);
                let side = i64::from(TILE);
                let texels = [
                    (cx * side).max(bounds[0]),
                    (cy * side).max(bounds[1]),
                    ((cx + 1) * side).min(bounds[2]),
                    ((cy + 1) * side).min(bounds[3]),
                ];
                if texels[2] <= texels[0] || texels[3] <= texels[1] {
                    continue;
                }
                let area = kurbo::Rect::new(
                    texels[0] as f64,
                    texels[1] as f64,
                    texels[2] as f64,
                    texels[3] as f64,
                );
                let mut deps = ContentHash::new();
                let mut cost = TileCost::default();
                let mut reads = false;
                for &index in indices {
                    let shape = &shapes[index as usize];
                    deps = deps.u64(hashes[index as usize]);
                    reads |= reads_paint(shape);
                    if let Some(ink) = boxes[index as usize] {
                        cost.pixels += ink.intersect(area).area();
                        cost.segments += shape.path.elements().len();
                        cost.shapes += 1;
                    }
                }
                if reads {
                    deps = deps.u64(inherited).u64(mapping.inherited_stroke.to_bits());
                }
                let texels = texels.map(|value| value as i32);
                let tile = TileKey {
                    linear: key.linear,
                    column: cx as i32,
                    row: cy as i32,
                    texels,
                    deps: deps.finish(),
                };
                let handle = match self.by_key.get(&tile) {
                    Some(&handle) => {
                        counter::bump(Counter::VectorLayerTilesReused);
                        handle
                    }
                    None => {
                        let handle = TILE_NAMESPACE | (self.next_handle & HANDLE_BITS);
                        self.next_handle = self.next_handle.wrapping_add(1);
                        fresh.push((handle, tile, slots.len() as u32));
                        handle
                    }
                };
                slots.push(Slot {
                    handle,
                    cell: [cx as i32, cy as i32],
                    texels,
                    path_rect: inverse.transform_rect_bbox(area),
                    shapes: indices.clone().into_boxed_slice(),
                    cost,
                });
            }
        }
        let source = Arc::new(TiledSource {
            id: self.next_id,
            key,
            shapes: Arc::clone(shapes),
            paint,
            linear,
            scale: mapping.scale,
            stroke_scale: mapping.stroke_scale,
            inherited_stroke: mapping.inherited_stroke,
            slots: slots.into_boxed_slice(),
        });
        self.next_id += 1;
        for slot in &source.slots {
            if let Some(raster) = self.rasters.get_mut(&slot.handle) {
                raster.owners += 1;
            }
        }
        // The deferrals of the cells of the drawing's last source at the same map.
        let owed: FxHashMap<[i32; 2], (u8, u32)> = previous
            .filter(|previous| previous.key.linear == key.linear)
            .map(|previous| {
                previous
                    .slots
                    .iter()
                    .filter_map(|slot| {
                        let raster = self.rasters.get(&slot.handle)?;
                        (raster.texels.is_none() && raster.deferred > 0)
                            .then_some((slot.cell, (raster.deferred, raster.deferred_at)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (handle, tile, slot) in fresh {
            let (deferred, deferred_at) = owed
                .get(&source.slots[slot as usize].cell)
                .copied()
                .unwrap_or((0, 0));
            self.by_key.insert(tile, handle);
            self.rasters.insert(
                handle,
                Raster {
                    key: tile,
                    source: Arc::clone(&source),
                    slot,
                    texels: None,
                    bytes: 0,
                    used: frame,
                    deferred,
                    deferred_at,
                    owners: 1,
                },
            );
        }
        self.ids.insert(source.id);
        self.sources.insert(
            key,
            SourceEntry {
                source: Arc::clone(&source),
                used: frame,
            },
        );
        Some(source)
    }

    /// Drops the sources no drawing asked for and no frame drew for [`HISTORY_FRAMES`] frames, and
    /// the rasters no source names.
    fn sweep(&mut self, frame: u32) {
        let old = |used: u32| frame.wrapping_sub(used) >= HISTORY_FRAMES;
        let stale: Vec<SourceKey> = self
            .sources
            .iter()
            .filter(|(_, entry)| {
                old(entry.used)
                    && entry.source.slots.iter().all(|slot| {
                        self.rasters
                            .get(&slot.handle)
                            .is_none_or(|raster| old(raster.used))
                    })
            })
            .map(|(key, _)| *key)
            .collect();
        for key in stale {
            let Some(entry) = self.sources.remove(&key) else {
                continue;
            };
            self.ids.remove(&entry.source.id);
            for slot in &entry.source.slots {
                let Some(raster) = self.rasters.get_mut(&slot.handle) else {
                    continue;
                };
                raster.owners = raster.owners.saturating_sub(1);
                if raster.owners > 0 {
                    continue;
                }
                self.bytes = self.bytes.saturating_sub(raster.bytes);
                let tile = raster.key;
                self.rasters.remove(&slot.handle);
                self.by_key.remove(&tile);
            }
        }
    }
}

/// A hash of one shape: its path's address and everything that paints it.
fn shape_hash(shape: &zgui_svg::Shape) -> u64 {
    let mut hash = ContentHash::new().u64(Arc::as_ptr(&shape.path).addr() as u64);
    match &shape.fill {
        None => hash = hash.u32(0),
        Some(fill) => {
            hash = paint_of(hash.u32(1), &fill.paint).u32(match fill.rule {
                zgui_scene::peniko::Fill::NonZero => 0,
                zgui_scene::peniko::Fill::EvenOdd => 1,
            });
        }
    }
    match &shape.stroke {
        None => hash = hash.u32(0),
        Some(stroke) => {
            let style = &stroke.style;
            hash = paint_of(hash.u32(1), &stroke.paint)
                .u64(style.width.to_bits())
                .u64(style.miter_limit.to_bits())
                .u32(match style.join {
                    kurbo::Join::Bevel => 0,
                    kurbo::Join::Miter => 1,
                    kurbo::Join::Round => 2,
                })
                .u32(cap_of(style.start_cap))
                .u32(cap_of(style.end_cap))
                .u64(style.dash_offset.to_bits());
            for dash in &style.dash_pattern {
                hash = hash.u64(dash.to_bits());
            }
        }
    }
    for clip in &shape.clips {
        hash = hash
            .u64(Arc::as_ptr(&clip.path).addr() as u64)
            .u32(match clip.rule {
                zgui_scene::peniko::Fill::NonZero => 0,
                zgui_scene::peniko::Fill::EvenOdd => 1,
            });
    }
    hash.finish()
}

/// A number for a stroke cap.
fn cap_of(cap: kurbo::Cap) -> u32 {
    match cap {
        kurbo::Cap::Butt => 0,
        kurbo::Cap::Square => 1,
        kurbo::Cap::Round => 2,
    }
}

/// `hash` followed by what `paint` is.
fn paint_of(hash: ContentHash, paint: &zgui_svg::Paint) -> ContentHash {
    let ink = |hash: ContentHash, ink: &zgui_svg::Ink| match ink {
        zgui_svg::Ink::Solid(color) => hash.u32(1).f32s(&color.to_premultiplied_srgb()),
        zgui_svg::Ink::Inherited { alpha } => hash.u32(2).f32(*alpha),
    };
    match paint {
        zgui_svg::Paint::Solid(solid) => ink(hash.u32(1), solid),
        zgui_svg::Paint::Gradient(gradient) => {
            let mut hash = hash.u32(2).u32(u32::from(gradient.repeating));
            hash = match gradient.kind {
                zgui_svg::GradientKind::Linear { start, end } => hash
                    .u32(0)
                    .u64(start.x.to_bits())
                    .u64(start.y.to_bits())
                    .u64(end.x.to_bits())
                    .u64(end.y.to_bits()),
                zgui_svg::GradientKind::Radial {
                    center,
                    radius_x,
                    radius_y,
                } => hash
                    .u32(1)
                    .u64(center.x.to_bits())
                    .u64(center.y.to_bits())
                    .u64(radius_x.to_bits())
                    .u64(radius_y.to_bits()),
            };
            for stop in &gradient.stops {
                hash = ink(hash.f32(stop.offset), &stop.color);
            }
            hash
        }
    }
}

impl VectorLayerCache {
    /// The tiles of a drawing past the size cap of one layer.
    ///
    /// No raster happens here. While the drawing changes scale, the tiles of the same drawing at
    /// the nearest scale within half to twice stand in, until the scale holds for
    /// [`SETTLE_FRAMES`] frames. A new source counts as a raster of the drawing: once the general
    /// rasteriser is built, three in [`DEMOTE_WINDOW`](super::DEMOTE_WINDOW) frames demote it as
    /// they demote a whole layer.
    pub(super) fn tiled(&mut self, request: &LayerRequest<'_>, mapping: &Mapping) -> LayerAnswer {
        let mut key = mapping.key;
        key.phase = [0, 0];
        let previous = self
            .histories
            .get(&request.owner)
            .and_then(|history| history.key)
            .filter(|previous| *previous != key)
            .map(|previous| SourceKey::of(&previous));
        let history = self.note_key(request.owner, Some(key));
        let source_key = SourceKey::of(&key);
        let frame = self.frame;
        if let Some(entry) = self.tiles.sources.get_mut(&source_key) {
            entry.used = frame;
            let source = Arc::clone(&entry.source);
            self.hits += 1;
            return LayerAnswer::Tiles {
                tiles: self.tiles.tiles_of(&source),
                provisional: false,
            };
        }
        if history.same
            && history.stable < SETTLE_FRAMES
            && let Some(near) = self.tiles.nearest(&source_key, mapping.scale)
        {
            self.hits += 1;
            return LayerAnswer::Tiles {
                tiles: self.tiles.tiles_of(&near),
                provisional: true,
            };
        }
        if self.raster_ready && frame < history.demoted_until {
            return LayerAnswer::Items(LayerFallback::Demoted);
        }
        let previous = previous
            .and_then(|previous| self.tiles.sources.get(&previous))
            .map(|entry| Arc::clone(&entry.source));
        match self
            .tiles
            .build(request, mapping, source_key, previous.as_deref(), frame)
        {
            Some(source) => {
                self.note_raster(request.owner);
                self.hits += 1;
                LayerAnswer::Tiles {
                    tiles: self.tiles.tiles_of(&source),
                    provisional: false,
                }
            }
            None => LayerAnswer::Items(LayerFallback::Ineligible),
        }
    }

    /// Whether the tiled source with number `id` still stands.
    pub(crate) fn tiles_alive(&self, id: u64) -> bool {
        self.tiles.alive(id)
    }

    /// Notes that the tile image `key` is shown at `admitted` on the surface this frame.
    pub(crate) fn shown(&mut self, key: ImageKey, admitted: Option<Rect<DevicePx, Device>>) {
        if let Some(handle) = handle_of(key) {
            self.tiles.shown.push((handle, admitted));
        }
    }

    /// Rasterises the tiles shown this frame where the frame needs them, hands each new raster to
    /// `write`, and returns the device rectangles of the tiles the frame needed and deferred.
    ///
    /// A tile is read where its clip admits it on the surface. A tile whose admitted part meets
    /// `damage` is rasterised when the frame's budget admits it, or when it was deferred on the
    /// [`DEFER_FRAMES`] frames before. Otherwise it stays transparent this frame and is owed one. A
    /// tile whose admitted part is within one tile of the damage is rasterised ahead only with
    /// budget that is left. Every other tile is not rasterised: the frame does not show it or does
    /// not redraw where it lies.
    pub(crate) fn settle(
        &mut self,
        viewport: Size<i32, Device>,
        damage: &DamageSet,
        write: &mut dyn FnMut(&LayerPixels),
    ) -> Vec<Rect<i32, Device>> {
        let shown = core::mem::take(&mut self.tiles.shown);
        if shown.is_empty() {
            return Vec::new();
        }
        let frame = self.frame;
        let surface = Rect::new(Point::new(0, 0), viewport);
        let grow = TILE as f32;
        let mut needed: Vec<u64> = Vec::new();
        let mut ahead: Vec<u64> = Vec::new();
        let mut owed_at: FxHashMap<u64, Vec<Rect<i32, Device>>> = FxHashMap::default();
        for (handle, admitted) in shown {
            let Some(raster) = self.tiles.rasters.get_mut(&handle) else {
                continue;
            };
            raster.used = frame;
            if raster.texels.is_some() {
                continue;
            }
            let pixels = admitted.and_then(|admitted| pixels(admitted).intersection(surface));
            let Some(pixels) = pixels else {
                continue;
            };
            let grown =
                admitted.and_then(|admitted| pixels_grown(admitted, grow).intersection(surface));
            if damage.intersects(pixels) {
                if !needed.contains(&handle) {
                    needed.push(handle);
                }
                owed_at.entry(handle).or_default().push(pixels);
            } else if grown.is_some_and(|grown| damage.intersects(grown))
                && !ahead.contains(&handle)
            {
                ahead.push(handle);
            }
        }
        let mut owed = Vec::new();
        for handle in needed {
            let Some(raster) = self.tiles.rasters.get(&handle) else {
                continue;
            };
            if raster.texels.is_some() {
                continue;
            }
            let us = raster.source.slots[raster.slot as usize].cost.us();
            let streak = raster.streak(frame);
            if streak >= DEFER_FRAMES || self.admits_tile(us) {
                self.raster_tile(handle, write);
                continue;
            }
            if let Some(raster) = self.tiles.rasters.get_mut(&handle) {
                raster.deferred = streak.saturating_add(1);
                raster.deferred_at = frame;
            }
            counter::bump(Counter::VectorLayerTilesDeferred);
            owed.extend(owed_at.remove(&handle).unwrap_or_default());
        }
        for handle in ahead {
            let Some(raster) = self.tiles.rasters.get(&handle) else {
                continue;
            };
            if raster.texels.is_some() {
                continue;
            }
            let us = raster.source.slots[raster.slot as usize].cost.us();
            if self.spent.us + us <= TILE_FRAME_US {
                self.raster_tile(handle, write);
            }
        }
        owed
    }

    /// Whether the frame's budget admits one more tile of `us`. The first of a frame always is.
    fn admits_tile(&self, us: f64) -> bool {
        self.spent.layers == 0 || self.spent.us + us <= TILE_FRAME_US
    }

    /// Rasterises the tile with `handle`, records it and hands it to `write`.
    fn raster_tile(&mut self, handle: u64, write: &mut dyn FnMut(&LayerPixels)) {
        let Some(raster) = self.tiles.rasters.get(&handle) else {
            return;
        };
        let source = Arc::clone(&raster.source);
        let slot = &source.slots[raster.slot as usize];
        let started = Instant::now();
        let [x0, y0, x1, y1] = slot.texels;
        let (width, height) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let mut texels = vec![0; width as usize * height as usize * 4];
        self.painter.paint(
            &LayerJob {
                shapes: &source.shapes,
                only: Some(&slot.shapes),
                paint: source.paint,
                map: Affine::translate((-f64::from(x0), -f64::from(y0))) * source.linear,
                stroke_scale: source.stroke_scale,
                inherited_stroke: source.inherited_stroke,
                width,
                height,
            },
            &mut texels,
        );
        let bytes = texels.len() as u64;
        let texels: Arc<[u8]> = Arc::from(texels);
        counter::bump(Counter::VectorLayerTilesRasterised);
        counter::add(
            Counter::VectorLayerRasterUs,
            started.elapsed().as_micros() as u64,
        );
        counter::add(Counter::VectorLayerBytesUploaded, bytes);
        self.spent.layers += 1;
        let us = slot.cost.us();
        self.spent.us += us;
        if !self.raster_ready {
            self.spent.cold_us += us;
        }
        let frame = self.frame;
        if let Some(raster) = self.tiles.rasters.get_mut(&handle) {
            if raster.texels.is_none() {
                self.tiles.bytes += bytes;
            }
            raster.texels = Some(Arc::clone(&texels));
            raster.bytes = bytes;
            raster.deferred = 0;
            raster.used = frame;
        }
        write(&LayerPixels {
            key: tile_key(handle),
            width,
            height,
            texels: Some(texels),
        });
    }

    /// Ends a frame for the tiles: drops the sources and rasters nothing used lately.
    pub(super) fn sweep_tiles(&mut self) {
        self.tiles.shown.clear();
        if !self.tiles.sources.is_empty() {
            self.tiles.sweep(self.frame);
        }
    }

    /// The rasterised tiles this frame did not draw, with the frame each was last drawn in.
    pub(super) fn cold_tiles(&self) -> Vec<(u32, u64)> {
        let frame = self.frame;
        self.tiles
            .held()
            .filter(|(_, _, used)| *used != frame)
            .map(|(handle, _, used)| (used, handle))
            .collect()
    }

    /// Drops the texels of the tile raster with `handle`, and returns their bytes.
    pub(super) fn evict_tile(&mut self, handle: u64) -> Option<u64> {
        self.tiles.lost(handle)
    }

    /// The bytes of the tiles this frame drew.
    pub(super) fn pinned_tile_bytes(&self) -> u64 {
        let frame = self.frame;
        self.tiles
            .held()
            .filter(|(_, _, used)| *used == frame)
            .map(|(_, bytes, _)| bytes)
            .sum()
    }
}

/// The device pixels `rect` touches.
fn pixels(rect: Rect<zgui_geom::DevicePx, Device>) -> Rect<i32, Device> {
    pixels_grown(rect, 0.0)
}

/// The device pixels `rect` grown by `by` on every side touches.
fn pixels_grown(rect: Rect<zgui_geom::DevicePx, Device>, by: f32) -> Rect<i32, Device> {
    let left = (rect.left().0 - by).floor();
    let top = (rect.top().0 - by).floor();
    let right = (rect.right().0 + by).ceil();
    let bottom = (rect.bottom().0 + by).ceil();
    if !(left.is_finite() && top.is_finite() && right.is_finite() && bottom.is_finite()) {
        return Rect::new(Point::new(0, 0), Size::new(0, 0));
    }
    let clamp = |value: f32| value.clamp(-1.0e9, 1.0e9) as i32;
    Rect::new(
        Point::new(clamp(left), clamp(top)),
        Size::new(clamp(right) - clamp(left), clamp(bottom) - clamp(top)),
    )
}

#[cfg(test)]
mod tests;
