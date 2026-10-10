//! Large drawings in tiles of the image pool, rasterised only where a frame shows them.
//!
//! A drawing whose raster is past the side or the byte cap of one layer is cut into tiles of
//! [`TILE`] texels in the raster space of its linear map. The origin of that space is the origin of
//! the path, so a translation moves every sprite and keeps every tile. Each tile is one colour
//! sprite that names its raster. The emit walk pushes the sprites and the record replays them.
//! [`VectorLayerCache::settle`] runs after the walk: it rasterises a tile only when the frame needs
//! it, within the frame's layer budget, and draws nothing for a tile the frame does not need.
//!
//! A tile's raster is keyed by what it shows: the linear map, its cell, and the shapes that meet
//! the cell, by path address and paint. An edit of one shape of a canvas makes a new source, and
//! every tile that the shape does not meet finds its raster again.

use std::sync::Arc;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};
use zgui_atlas::{Atlas, AtlasKey, AtlasTile, TextureKind};
use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_profile::{Counter, counter};
use zgui_scene::kurbo::{self, Affine};
use zgui_scene::{ContentHash, ResourceGeneration, ResourceKey, Scene, Settle};

use super::{
    COLD_US, DEFER_FRAMES, HISTORY_FRAMES, LayerAnswer, LayerFallback, LayerKey, LayerRequest,
    Mapping, SETTLE_FRAMES, US_PER_PIXEL, US_PER_SEGMENT, VectorLayerCache, paint_hash, reach,
};
use crate::content::vectors::cpu::{LayerJob, VectorPainter};
use crate::emit::vector::ShapePaint;

/// The side of one tile, in texels.
pub(crate) const TILE: u32 = 512;

/// The handle namespace of tile rasters in the image pool.
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
    /// A hash of the shapes that meet the cell, in painting order, and the paint they read.
    deps: u64,
}

/// One tile of a source.
#[derive(Debug)]
pub(crate) struct Slot {
    /// The handle of its raster.
    handle: u64,
    /// Its texels in the raster space of the linear map, as `[x0, y0, x1, y1]`.
    texels: [i32; 4],
    /// The same rectangle in path space.
    path_rect: kurbo::Rect,
    /// The indices of the shapes that meet it, in painting order.
    shapes: Box<[u32]>,
    /// Its estimated microseconds.
    us: f64,
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
    /// The lifetime of the cache that names the rasters.
    generation: ResourceGeneration,
}

/// The tiles of one drawing, as the layer cache answers them.
#[derive(Clone, Debug)]
pub struct Tiles(pub(crate) Arc<TiledSource>);

impl PartialEq for Tiles {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Tiles {
    /// The number of the source, which a paint record keeps to know it still stands.
    pub fn id(&self) -> u64 {
        self.0.id
    }

    /// How many tiles the drawing has.
    pub fn len(&self) -> usize {
        self.0.slots.len()
    }

    /// Whether the drawing has no tile.
    pub fn is_empty(&self) -> bool {
        self.0.slots.is_empty()
    }

    /// Each tile's name and its rectangle in path space.
    pub fn sprites(&self) -> impl Iterator<Item = (ResourceKey, kurbo::Rect)> + '_ {
        let generation = self.0.generation;
        self.0.slots.iter().map(move |slot| {
            (
                ResourceKey::of(tile_key(slot.handle), generation),
                slot.path_rect,
            )
        })
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
    /// Whether its texels are in the atlas.
    rasterised: bool,
    /// Its bytes while rasterised.
    bytes: u64,
    /// The frame it was last drawn in.
    used: u32,
    /// How many frames in a row it was needed and deferred.
    deferred: u8,
    /// How many sources name it.
    owners: u32,
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
        }
    }
}

/// The atlas key of the raster with `handle`.
fn tile_key(handle: u64) -> AtlasKey {
    AtlasKey::new(handle, TextureKind::Image)
}

/// Whether `name` names a tile raster.
fn is_tile(name: ResourceKey) -> bool {
    name.kind() == TextureKind::Image && name.hash() & NAMESPACE_BITS == TILE_NAMESPACE
}

impl TileCache {
    /// The bytes of every rasterised tile.
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// How many tiles are rasterised.
    pub(crate) fn rasterised(&self) -> usize {
        self.rasters
            .values()
            .filter(|raster| raster.rasterised)
            .count()
    }

    /// Every rasterised tile's atlas key, its bytes and the frame it was last drawn in.
    fn held(&self) -> impl Iterator<Item = (u64, AtlasKey, u64, u32)> + '_ {
        self.rasters
            .iter()
            .filter(|(_, raster)| raster.rasterised)
            .map(|(handle, raster)| (*handle, tile_key(*handle), raster.bytes, raster.used))
    }

    /// Notes that the atlas no longer holds the raster with `handle`.
    fn lost(&mut self, handle: u64) {
        if let Some(raster) = self.rasters.get_mut(&handle)
            && raster.rasterised
        {
            raster.rasterised = false;
            self.bytes = self.bytes.saturating_sub(raster.bytes);
            raster.bytes = 0;
        }
    }

    /// Whether a source with number `id` stands.
    pub(crate) fn alive(&self, id: u64) -> bool {
        self.ids.contains(&id)
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
    /// raster. `None` when the drawing inks nothing or has more than [`MAX_TILES`] tiles.
    fn build(
        &mut self,
        request: &LayerRequest<'_>,
        mapping: &Mapping,
        key: SourceKey,
        frame: u32,
        generation: ResourceGeneration,
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
                let mut us = 0.0;
                let mut reads = false;
                for &index in indices {
                    let shape = &shapes[index as usize];
                    deps = deps.u64(hashes[index as usize]);
                    reads |= reads_paint(shape);
                    if let Some(ink) = boxes[index as usize] {
                        us += US_PER_PIXEL * ink.intersect(area).area()
                            + US_PER_SEGMENT * shape.path.elements().len() as f64;
                    }
                }
                if reads {
                    deps = deps.u64(inherited).u64(mapping.inherited_stroke.to_bits());
                }
                let tile = TileKey {
                    linear: key.linear,
                    column: cx as i32,
                    row: cy as i32,
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
                    texels: texels.map(|value| value as i32),
                    path_rect: inverse.transform_rect_bbox(area),
                    shapes: indices.clone().into_boxed_slice(),
                    us,
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
            generation,
        });
        self.next_id += 1;
        for slot in &source.slots {
            if let Some(raster) = self.rasters.get_mut(&slot.handle) {
                raster.owners += 1;
            }
        }
        for (handle, tile, slot) in fresh {
            self.by_key.insert(tile, handle);
            self.rasters.insert(
                handle,
                Raster {
                    key: tile,
                    source: Arc::clone(&source),
                    slot,
                    rasterised: false,
                    bytes: 0,
                    used: frame,
                    deferred: 0,
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
    fn sweep(&mut self, atlas: &mut Atlas, frame: u32) {
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
                if raster.rasterised && atlas.remove_if_unreferenced(tile_key(slot.handle)) {
                    self.bytes = self.bytes.saturating_sub(raster.bytes);
                }
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
    /// [`SETTLE_FRAMES`] frames.
    pub(super) fn tiled(&mut self, request: &LayerRequest<'_>, mapping: &Mapping) -> LayerAnswer {
        let mut key = mapping.key;
        key.phase = [0, 0];
        let history = self.note_key(request.owner, Some(key));
        let source_key = SourceKey::of(&key);
        let frame = self.frame;
        if let Some(entry) = self.tiles.sources.get_mut(&source_key) {
            entry.used = frame;
            self.hits += 1;
            return LayerAnswer::Tiles {
                tiles: Tiles(Arc::clone(&entry.source)),
                provisional: false,
            };
        }
        if history.same
            && history.stable < SETTLE_FRAMES
            && let Some(near) = self.tiles.nearest(&source_key, mapping.scale)
        {
            self.hits += 1;
            return LayerAnswer::Tiles {
                tiles: Tiles(near),
                provisional: true,
            };
        }
        let generation = self.generation;
        match self
            .tiles
            .build(request, mapping, source_key, frame, generation)
        {
            Some(source) => {
                self.hits += 1;
                LayerAnswer::Tiles {
                    tiles: Tiles(source),
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

    /// Answers every tile sprite the walk left named, and returns the device rectangles of the
    /// tiles the frame needed and deferred.
    ///
    /// A tile drawn from a raster in the atlas is placed. A tile whose sprite meets `damage` is
    /// rasterised when the frame's budget admits it, or when it was deferred on
    /// [`DEFER_FRAMES`] frames; otherwise it draws nothing this frame and is owed one. A tile
    /// within one tile of the damage is rasterised ahead only with budget that is left. Every
    /// other tile draws nothing: the frame does not redraw where it lies.
    pub(crate) fn settle(
        &mut self,
        atlas: &mut Atlas,
        scene: &mut Scene,
        damage: &DamageSet,
        evict: &mut dyn FnMut(&mut Atlas) -> Vec<AtlasKey>,
    ) -> Vec<Rect<i32, Device>> {
        let named = scene.named_color_sprites();
        if !named.iter().any(|(name, _)| is_tile(*name)) {
            return Vec::new();
        }
        let frame = self.frame;
        let viewport = scene.viewport();
        let surface = Rect::new(Point::new(0, 0), viewport);
        let grow = TILE as f32;
        let mut answers: FxHashMap<u64, Option<AtlasTile>> = FxHashMap::default();
        let mut needed: Vec<u64> = Vec::new();
        let mut ahead: Vec<u64> = Vec::new();
        let mut seen: Vec<u64> = Vec::new();
        let mut owed_at: FxHashMap<u64, Vec<Rect<i32, Device>>> = FxHashMap::default();
        for (name, on_device) in named {
            if !is_tile(name) {
                continue;
            }
            let handle = name.hash();
            let known =
                name.generation() == self.generation && self.tiles.rasters.contains_key(&handle);
            if !known {
                answers.insert(handle, None);
                continue;
            }
            let pixels = pixels(on_device).intersection(surface);
            let grown = pixels_grown(on_device, grow).intersection(surface);
            if let Some(raster) = self.tiles.rasters.get_mut(&handle) {
                raster.used = frame;
            }
            if let Some(pixels) = pixels.filter(|pixels| damage.intersects(*pixels)) {
                if !needed.contains(&handle) {
                    needed.push(handle);
                }
                owed_at.entry(handle).or_default().push(pixels);
            } else if grown.is_some_and(|grown| damage.intersects(grown)) {
                if !ahead.contains(&handle) {
                    ahead.push(handle);
                }
            } else if !seen.contains(&handle) {
                seen.push(handle);
            }
        }
        // Whatever is in the atlas is placed, wherever it lies.
        for &handle in needed.iter().chain(&ahead).chain(&seen) {
            if answers.contains_key(&handle) {
                continue;
            }
            let rasterised = self
                .tiles
                .rasters
                .get(&handle)
                .is_some_and(|raster| raster.rasterised);
            if !rasterised {
                continue;
            }
            match atlas.get(tile_key(handle)) {
                Some(tile) => {
                    answers.insert(handle, Some(tile));
                }
                None => self.tiles.lost(handle),
            }
        }
        let mut owed = Vec::new();
        for handle in needed {
            if answers.contains_key(&handle) {
                continue;
            }
            let Some(raster) = self.tiles.rasters.get(&handle) else {
                continue;
            };
            let us = raster.source.slots[raster.slot as usize].us;
            let forced = raster.deferred >= DEFER_FRAMES;
            if forced || self.admits_tile(us) {
                let tile = self.raster_tile(atlas, handle, evict);
                answers.insert(handle, tile);
                continue;
            }
            if let Some(raster) = self.tiles.rasters.get_mut(&handle) {
                raster.deferred = raster.deferred.saturating_add(1);
            }
            counter::bump(Counter::VectorLayerTilesDeferred);
            owed.extend(owed_at.remove(&handle).unwrap_or_default());
            answers.insert(handle, None);
        }
        for handle in ahead {
            if answers.contains_key(&handle) {
                continue;
            }
            let Some(raster) = self.tiles.rasters.get(&handle) else {
                continue;
            };
            let us = raster.source.slots[raster.slot as usize].us;
            if self.spent.us + us <= TILE_FRAME_US {
                let tile = self.raster_tile(atlas, handle, evict);
                answers.insert(handle, tile);
            }
        }
        scene.settle_named(|name, _| {
            if !is_tile(name) {
                return Settle::Keep;
            }
            match answers.get(&name.hash()) {
                Some(Some(tile)) => Settle::Place(*tile),
                _ => Settle::Blank,
            }
        });
        owed
    }

    /// Whether the frame's budget admits one more tile of `us`. The first of a frame always is.
    fn admits_tile(&self, us: f64) -> bool {
        self.spent.layers == 0 || self.spent.us + us <= TILE_FRAME_US
    }

    /// Rasterises the tile with `handle` into the atlas, and records it.
    fn raster_tile(
        &mut self,
        atlas: &mut Atlas,
        handle: u64,
        evict: &mut dyn FnMut(&mut Atlas) -> Vec<AtlasKey>,
    ) -> Option<AtlasTile> {
        let raster = self.tiles.rasters.get(&handle)?;
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
        let texels = Arc::new(texels);
        let key = tile_key(handle);
        let size = Size::new(width as i32, height as i32);
        let tile = match atlas.get_or_insert(key, size, || Arc::clone(&texels)) {
            Ok(tile) => tile,
            Err(_) => {
                let removed = evict(atlas);
                self.forget_tiles(&removed);
                atlas
                    .get_or_insert(key, size, || Arc::clone(&texels))
                    .ok()?
            }
        };
        counter::bump(Counter::VectorLayerTilesRasterised);
        counter::add(
            Counter::VectorLayerRasterUs,
            started.elapsed().as_micros() as u64,
        );
        counter::add(Counter::VectorLayerBytesUploaded, bytes);
        self.spent.layers += 1;
        self.spent.us += slot.us;
        if !self.raster_ready {
            self.spent.cold_us += slot.us;
        }
        let frame = self.frame;
        if let Some(raster) = self.tiles.rasters.get_mut(&handle) {
            if !raster.rasterised {
                self.tiles.bytes += bytes;
            }
            raster.rasterised = true;
            raster.bytes = bytes;
            raster.deferred = 0;
            raster.used = frame;
        }
        Some(tile)
    }

    /// Ends a frame for the tiles: drops the sources and rasters nothing used lately.
    pub(super) fn sweep_tiles(&mut self, atlas: &mut Atlas) {
        if !self.tiles.sources.is_empty() {
            self.tiles.sweep(atlas, self.frame);
        }
    }

    /// Notes that the atlas removed `removed`, for the tile rasters among them.
    pub(super) fn forget_tile_rasters(&mut self, removed: &FxHashSet<AtlasKey>) {
        let lost: Vec<u64> = removed
            .iter()
            .filter(|key| {
                key.kind() == TextureKind::Image && key.handle() & NAMESPACE_BITS == TILE_NAMESPACE
            })
            .map(|key| key.handle())
            .collect();
        for handle in lost {
            self.tiles.lost(handle);
        }
    }

    /// The rasterised tiles nothing pins, with their bytes and the frame each was last drawn in.
    pub(super) fn cold_tiles(&self, atlas: &Atlas) -> Vec<(u32, u64, u64)> {
        let frame = self.frame;
        self.tiles
            .held()
            .filter(|(_, key, _, used)| *used != frame && !super::pinned(atlas, *key))
            .map(|(handle, _, bytes, used)| (used, handle, bytes))
            .collect()
    }

    /// Removes the tile raster with `handle` when nothing holds it, and reports its bytes.
    pub(super) fn evict_tile(&mut self, atlas: &mut Atlas, handle: u64) -> Option<u64> {
        if !atlas.remove_if_unreferenced(tile_key(handle)) {
            return None;
        }
        let bytes = self
            .tiles
            .rasters
            .get(&handle)
            .map_or(0, |raster| raster.bytes);
        self.tiles.lost(handle);
        Some(bytes)
    }

    /// The bytes of the tiles this frame drew.
    pub(super) fn pinned_tile_bytes(&self, atlas: &Atlas) -> u64 {
        let frame = self.frame;
        self.tiles
            .held()
            .filter(|(_, key, _, used)| *used == frame || atlas.used_this_frame(*key))
            .map(|(_, _, bytes, _)| bytes)
            .sum()
    }

    /// Every rasterised tile's atlas key and bytes.
    pub(super) fn tile_entries(&self) -> impl Iterator<Item = (AtlasKey, u64)> + '_ {
        self.tiles.held().map(|(_, key, bytes, _)| (key, bytes))
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
