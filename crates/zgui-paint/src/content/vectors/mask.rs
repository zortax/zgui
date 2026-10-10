//! Small solid vector shapes rasterised into the shared monochrome atlas.
//!
//! The cache deliberately names geometry rather than colour. A monochrome tile is coverage, so
//! recolouring an icon only changes the sprite instance and never rasterises the outline again.

use core::hash::{Hash, Hasher};

use rustc_hash::{FxHashMap, FxHasher};
use zgui_atlas::{Atlas, AtlasKey, AtlasTile, TextureKind};
use zgui_geom::{Device, Rect, Size};
use zgui_profile::{Counter, counter};
use zgui_scene::kurbo::{self, BezPath, PathEl};
use zgui_scene::{VectorId, peniko};

#[cfg(test)]
mod tests;

/// How a path's coverage is produced.
#[derive(Clone, Copy, Debug)]
pub enum VectorMaskStyle<'a> {
    /// The path's interior under this fill rule.
    Fill(peniko::Fill),
    /// The outline produced by this stroke style.
    Stroke(&'a kurbo::Stroke),
}

/// The geometry needed to request one coverage mask.
#[derive(Clone, Copy, Debug)]
pub struct VectorMaskRequest<'a> {
    /// The shape that asks, as the emit walk names it.
    ///
    /// The cache keeps a short history per owner. A shape that changes in most frames stops
    /// taking the mask route, because each change costs a raster and a tile.
    pub owner: VectorId,
    /// The outline in its own space.
    pub path: &'a BezPath,
    /// What places the outline in the coordinates its own box is measured in.
    pub placement: zgui_scene::kurbo::Affine,
    /// Whether the outline is filled or stroked.
    pub style: VectorMaskStyle<'a>,
    /// Mask texels per unit of each of the path's own axes.
    ///
    /// One and one where the shape is drawn under nothing but a translation, which is the ordinary
    /// case and the one that makes a tile shared between two placements of the same icon. A shape
    /// under a scale is rasterised at the density the scale asks for, so what reaches the screen is
    /// coverage the rasteriser produced rather than coverage a sampler stretched. A rotation is a
    /// density of one on both axes, so every angle of a turning shape shares one tile.
    pub density: [f32; 2],
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// Integer pixel bounds of the raster, in the same space the density measures.
    pub bounds: Rect<i32, Device>,
}

/// One cached coverage raster and the key that keeps it alive across replay.
#[derive(Clone, Copy, Debug)]
pub struct VectorMask {
    /// Where the coverage lives now.
    pub tile: AtlasTile,
    /// What the atlas knows the coverage as.
    pub key: AtlasKey,
}

/// Paint-side source of small vector coverage masks, and the route history of each shape.
pub trait VectorMaskSource {
    /// Returns a cached mask, rasterising it on a miss.
    fn vector_mask(&self, request: VectorMaskRequest<'_>) -> Option<VectorMask>;

    /// Whether `owner` may try the analytic route this frame.
    ///
    /// False for a few frames after a large shape failed recognition, so a path that is no
    /// analytic shape is not measured again on every frame.
    fn analytic(&self, _owner: VectorId) -> bool {
        true
    }

    /// Records that a large shape of `owner` failed recognition.
    fn analytic_declined(&self, _owner: VectorId) {}

    /// Whether `owner` may try the marks route this frame.
    ///
    /// False for a few frames after a large shape failed recognition there.
    fn marks(&self, _owner: VectorId) -> bool {
        true
    }

    /// Records that a large shape of `owner` failed recognition on the marks route.
    fn marks_declined(&self, _owner: VectorId) {}

    /// Where recognitions are kept between frames, if anywhere.
    #[doc(hidden)]
    fn recognitions(&self) -> Option<core::cell::RefMut<'_, super::recognitions::Recognitions>> {
        None
    }

    /// Where mark payloads are kept between frames, if anywhere.
    #[doc(hidden)]
    fn payloads(&self) -> Option<core::cell::RefMut<'_, super::payloads::MarkPayloads>> {
        None
    }

    /// Where whole drawings are rasterised on the CPU, if anywhere.
    #[doc(hidden)]
    fn layers(&self) -> Option<&dyn super::layer::VectorLayerSource> {
        None
    }
}

/// A source that declines every mask request, the analytic route and the marks route.
///
/// Every shape drawn through it takes the general route.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoVectorMasks;

impl VectorMaskSource for NoVectorMasks {
    fn vector_mask(&self, _request: VectorMaskRequest<'_>) -> Option<VectorMask> {
        None
    }

    fn analytic(&self, _owner: VectorId) -> bool {
        false
    }

    fn marks(&self, _owner: VectorId) -> bool {
        false
    }
}

/// A source that declines every mask request and the marks route, and allows the analytic route.
///
/// It remembers nothing, so a shape that fails recognition is measured again on every frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnalyticOnly;

impl VectorMaskSource for AnalyticOnly {
    fn vector_mask(&self, _request: VectorMaskRequest<'_>) -> Option<VectorMask> {
        None
    }

    fn marks(&self, _owner: VectorId) -> bool {
        false
    }
}

/// A source that declines every mask request and the analytic route, and allows the marks route.
///
/// It remembers nothing, so a shape that fails recognition is measured again on every frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct MarksOnly;

impl VectorMaskSource for MarksOnly {
    fn vector_mask(&self, _request: VectorMaskRequest<'_>) -> Option<VectorMask> {
        None
    }

    fn analytic(&self, _owner: VectorId) -> bool {
        false
    }
}

/// A source that declines every mask request and the analytic route, allows the marks route, and
/// keeps recognitions and mark payloads between frames. For tests.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct CachedMarks {
    /// The recognitions.
    pub(crate) recognitions: core::cell::RefCell<super::recognitions::Recognitions>,
    /// The mark payloads.
    pub(crate) payloads: core::cell::RefCell<super::payloads::MarkPayloads>,
}

impl CachedMarks {
    /// A source holding nothing yet, in its first frame.
    pub fn new() -> Self {
        let source = Self::default();
        source.recognitions.borrow_mut().begin_frame();
        source.payloads.borrow_mut().begin_frame();
        source
    }

    /// Ends a frame and starts the next.
    pub fn end_frame(&self) {
        let mut recognitions = self.recognitions.borrow_mut();
        recognitions.end_frame();
        recognitions.begin_frame();
        let mut payloads = self.payloads.borrow_mut();
        payloads.end_frame();
        payloads.begin_frame();
    }
}

impl VectorMaskSource for CachedMarks {
    fn vector_mask(&self, _request: VectorMaskRequest<'_>) -> Option<VectorMask> {
        None
    }

    fn analytic(&self, _owner: VectorId) -> bool {
        false
    }

    fn recognitions(&self) -> Option<core::cell::RefMut<'_, super::recognitions::Recognitions>> {
        Some(self.recognitions.borrow_mut())
    }

    fn payloads(&self) -> Option<core::cell::RefMut<'_, super::payloads::MarkPayloads>> {
        Some(self.payloads.borrow_mut())
    }
}

/// Geometry identity independent of its integer translation and tint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Fingerprint {
    commands: Box<[Command]>,
    style: Style,
    scale: u32,
    size: [i32; 2],
}

/// Raster style encoded without borrowing the source shape.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Style {
    Fill(bool),
    Stroke {
        width: u32,
        join: u8,
        miter_limit: u32,
        start_cap: u8,
        end_cap: u8,
        dashes: Box<[u32]>,
        dash_offset: u32,
    },
}

/// One path command encoded in exact `f32` bits after integer-origin normalisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Command {
    Move([u32; 2]),
    Line([u32; 2]),
    Quad([u32; 4]),
    Curve([u32; 6]),
    Close,
}

/// The route one part of a shape took the last time it asked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Routed {
    /// It has not asked yet.
    #[default]
    None,
    /// It got a mask.
    Mask,
    /// It was declined and took the general route.
    Declined,
}

/// What the cache remembers about one owner across frames.
#[derive(Clone, Copy, Debug, Default)]
struct RouteHistory {
    /// Bit `n` is set when the owner's request changed `n` frames ago. Bit 0 is this frame.
    changes: u8,
    /// The frame `changes` is shifted to.
    frame: u32,
    /// Whether the owner changes too often for the mask route.
    volatile: bool,
    /// The last stamp of each part, fill and stroke. Zero means none seen.
    stamps: [u64; 2],
    /// The route each part took last.
    routed: [Routed; 2],
    /// The tiles each part drew last, the current one first.
    tiles: [[Option<AtlasKey>; 2]; 2],
    /// The frame a large shape of the owner last failed recognition in.
    analytic_declined: Option<u32>,
    /// The frame a large shape of the owner last failed recognition on the marks route in.
    marks_declined: Option<u32>,
}

impl RouteHistory {
    /// Shifts the change register to `frame`. A frame that did not touch the owner is stable.
    fn advance(&mut self, frame: u32) {
        let gap = frame.wrapping_sub(self.frame);
        self.changes = if gap >= u8::BITS {
            0
        } else {
            self.changes << gap
        };
        self.frame = frame;
    }

    /// Records the stamp `part` asks with this frame, and reports whether it moved.
    fn observe(&mut self, part: usize, stamp: u64) -> bool {
        let moved = self.stamps[part] != 0 && self.stamps[part] != stamp;
        self.stamps[part] = stamp;
        moved
    }

    /// Records a change in this frame when `changed`, and updates the volatile flag.
    fn note(&mut self, changed: bool) {
        if changed {
            self.changes |= 1;
        }
        let recent = self.changes & RECENT;
        if recent.count_ones() >= VOLATILE_CHANGES {
            self.volatile = true;
        } else if recent == 0 {
            self.volatile = false;
        }
    }

    /// Makes `key` the current tile of `part`.
    ///
    /// The tile before it stays tracked, so a shape that toggles between two geometries keeps
    /// hitting. The one before that is dropped. It goes to `superseded` when the owner changed at
    /// least twice in eight frames, and to eviction otherwise: an icon swapped once may swap back.
    fn track(&mut self, part: usize, key: AtlasKey, superseded: &mut Vec<AtlasKey>) {
        let tiles = &mut self.tiles[part];
        if tiles[0] == Some(key) {
            return;
        }
        if tiles[1] == Some(key) {
            tiles.swap(0, 1);
            return;
        }
        let dropped = tiles[1];
        *tiles = [Some(key), tiles[0]];
        if let Some(dropped) = dropped
            && self.changes.count_ones() >= RECLAIM_CHANGES
        {
            superseded.push(dropped);
        }
    }

    /// Gives back every tile `part` tracks.
    fn give_back(&mut self, part: usize, superseded: &mut Vec<AtlasKey>) {
        superseded.extend(self.tiles[part].iter_mut().filter_map(Option::take));
    }

    /// Records the route `part` took, and counts a change between the mask and a decline.
    fn route(&mut self, part: usize, routed: Routed) {
        let was = self.routed[part];
        if was != Routed::None && routed != Routed::None && was != routed {
            counter::bump(Counter::VectorTierChanges);
        }
        self.routed[part] = routed;
    }
}

/// The frames the volatile test reads: this one and the three before it.
const RECENT: u8 = 0b1111;

/// How many changes in [`RECENT`] make an owner volatile.
const VOLATILE_CHANGES: u32 = 3;

/// How many changes in eight frames make an owner's dropped tiles worth removing at once.
const RECLAIM_CHANGES: u32 = 2;

/// How many frames a history survives without a request.
const HISTORY_FRAMES: u32 = 8;

/// The most segments a volatile shape may have and keep the mask while vector raster is cold.
///
/// Past this a raster per frame costs more than the general rasteriser does once it is built.
const COLD_SEGMENTS: usize = 4096;

/// The most new masks one frame rasterises while the general rasteriser is built.
const BUDGET_TILES: u32 = 32;

/// The most texels those new masks may cover together.
const BUDGET_TEXELS: u64 = 128 * 1024;

/// The most new masks of volatile shapes one frame rasterises while the general rasteriser is
/// cold.
///
/// A volatile shape rasterises again in most frames. Past this the frame declines the rest, and a
/// decline builds the general rasteriser, which then takes every volatile shape.
const COLD_BUDGET_TILES: u32 = 256;

/// The most texels those new masks may cover together.
const COLD_BUDGET_TEXELS: u64 = 1024 * 1024;

/// What one frame has spent on new masks.
#[derive(Clone, Copy, Debug, Default)]
struct Budget {
    /// Masks rasterised.
    tiles: u32,
    /// Texels they cover.
    texels: u64,
}

impl Budget {
    /// Whether one more mask of `texels` stays within `tiles` masks and `limit` texels.
    fn admits(&self, texels: u64, tiles: u32, limit: u64) -> bool {
        self.tiles < tiles && self.texels + texels <= limit
    }

    /// Records one mask of `texels`.
    fn spend(&mut self, texels: u64) {
        self.tiles += 1;
        self.texels += texels;
    }
}

/// The sparse metadata beside monochrome atlas entries used by vector masks.
#[derive(Debug)]
pub(crate) struct VectorMaskCache {
    entries: FxHashMap<Fingerprint, AtlasKey>,
    next_handle: u64,
    /// The recent requests of each owner.
    histories: FxHashMap<VectorId, RouteHistory>,
    /// The current frame, advanced by [`VectorMaskCache::begin_frame`].
    frame: u32,
    /// Whether the general vector rasteriser is built and can take a declined shape at no setup
    /// cost.
    raster_ready: bool,
    /// Tiles their owners stopped drawing, removed when the frame ends.
    superseded: Vec<AtlasKey>,
    /// What this frame has spent on new masks.
    budget: Budget,
    /// What this frame has spent on new masks of volatile shapes while the general rasteriser is
    /// cold.
    cold: Budget,
    /// What recognition found in the paths drawn lately.
    pub(crate) recognitions: super::recognitions::Recognitions,
    /// The mark payloads lowered lately.
    pub(crate) payloads: super::payloads::MarkPayloads,
}

/// A disjoint namespace from glyph handles in the monochrome atlas.
const MASK_NAMESPACE: u64 = 0x7E00_0000_0000_0000;
const HANDLE_BITS: u64 = 0x00FF_FFFF_FFFF_FFFF;

impl Default for VectorMaskCache {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            next_handle: MASK_NAMESPACE,
            histories: FxHashMap::default(),
            frame: 0,
            raster_ready: false,
            superseded: Vec::new(),
            budget: Budget::default(),
            cold: Budget::default(),
            recognitions: super::recognitions::Recognitions::default(),
            payloads: super::payloads::MarkPayloads::default(),
        }
    }
}

impl VectorMaskCache {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.budget = Budget::default();
        self.cold = Budget::default();
        self.recognitions.begin_frame();
        self.payloads.begin_frame();
    }

    /// Sets whether the general vector rasteriser is built.
    pub(crate) fn set_raster_ready(&mut self, ready: bool) {
        self.raster_ready = ready;
    }

    /// Ends a frame, and reports how many tiles it removed.
    ///
    /// Removes the superseded tiles that nothing holds and this frame did not draw, then forgets
    /// the owners no request touched for [`HISTORY_FRAMES`] frames. Call it after the frame's
    /// uploads are flushed: a removal discards the tile's queued upload.
    ///
    /// Only mask tiles are ever superseded, so glyphs and images are never removed here.
    pub(crate) fn end_frame(&mut self, atlas: &mut Atlas) -> usize {
        let mut removed = Vec::new();
        for key in self.superseded.drain(..) {
            if !atlas.used_this_frame(key) && atlas.remove_if_unreferenced(key) {
                removed.push(key);
            }
        }
        self.forget_tiles(&removed);
        let frame = self.frame;
        self.histories
            .retain(|_, history| frame.wrapping_sub(history.frame) < HISTORY_FRAMES);
        self.recognitions.end_frame();
        self.payloads.end_frame();
        removed.len()
    }

    /// Looks up or builds one coverage tile, or declines the request.
    ///
    /// A request changes when its stamp moved and it needs a tile its owner does not track. An
    /// owner whose request changed in three of the last four frames is volatile. A volatile
    /// owner is declined while the general rasteriser is built. While it is cold, only a shape of
    /// more than [`COLD_SEGMENTS`] segments is declined, because a decline then builds it.
    ///
    /// While the general rasteriser is built, a frame also rasterises at most [`BUDGET_TILES`] new
    /// masks covering [`BUDGET_TEXELS`], and declines the rest. While it is cold, a frame
    /// rasterises at most [`COLD_BUDGET_TILES`] new masks of volatile owners covering
    /// [`COLD_BUDGET_TEXELS`], and declines the rest of those owners. A hit costs nothing.
    pub(crate) fn tile_for(
        &mut self,
        atlas: &mut Atlas,
        request: VectorMaskRequest<'_>,
    ) -> Option<VectorMask> {
        let width = request.bounds.size.width;
        let height = request.bounds.size.height;
        if width <= 0 || height <= 0 {
            return None;
        }
        let Self {
            entries,
            next_handle,
            histories,
            frame,
            raster_ready,
            superseded,
            budget,
            cold,
            recognitions: _,
            payloads: _,
        } = self;
        let part = part_of(request.style);
        let history = histories
            .entry(request.owner)
            .or_insert_with(|| RouteHistory {
                frame: *frame,
                ..RouteHistory::default()
            });
        history.advance(*frame);
        let moved = history.observe(part, stamp(&request, part));
        let decline = |history: &mut RouteHistory, superseded: &mut Vec<AtlasKey>| {
            history.give_back(part, superseded);
            history.route(part, Routed::Declined);
            None
        };
        // A volatile owner is declined before its geometry is read, so a moved stamp alone counts.
        if history.volatile {
            history.note(moved);
            if history.volatile && declines(*raster_ready, request.path) {
                return decline(history, superseded);
            }
        }
        let commands = commands(
            request.path,
            request.placement,
            request.density,
            request.bounds.origin.x,
            request.bounds.origin.y,
        )?;
        // The density needs no field of its own. It is already in the scaled commands, in the size
        // they were measured to produce and in the scaled stroke width, so two requests that agree
        // on all three ask for the same raster whatever densities they arrived at it by.
        let fingerprint = Fingerprint {
            commands: commands.into_boxed_slice(),
            style: style(request.style, request.density[0])?,
            scale: request.scale.to_bits(),
            size: [width, height],
        };

        let held = entries.get(&fingerprint).copied();
        // A stable owner changes only when it needs a tile it does not track. A drawing placed
        // again on whole pixels, as a scrolled one is, keeps its tile and is not a change.
        if !history.volatile {
            let tracked = held.is_some_and(|key| history.tiles[part].contains(&Some(key)));
            history.note(moved && !tracked);
            if history.volatile && declines(*raster_ready, request.path) {
                return decline(history, superseded);
            }
        }
        let texels = u64::from(width.unsigned_abs()) * u64::from(height.unsigned_abs());
        let fresh = |atlas: &Atlas| !held.is_some_and(|key| atlas.contains(key));
        if *raster_ready && !budget.admits(texels, BUDGET_TILES, BUDGET_TEXELS) && fresh(atlas) {
            counter::bump(Counter::VectorMaskBudgetOverflow);
            history.route(part, Routed::Declined);
            return None;
        }
        // The volatile owner asks again next frame, so its raster is a cost of every frame.
        let volatile_cold = !*raster_ready && history.volatile;
        if volatile_cold
            && !cold.admits(texels, COLD_BUDGET_TILES, COLD_BUDGET_TEXELS)
            && fresh(atlas)
        {
            counter::bump(Counter::VectorMaskBudgetOverflow);
            return decline(history, superseded);
        }
        let key = if let Some(key) = held {
            key
        } else {
            let key = fresh_key(next_handle, atlas);
            entries.insert(fingerprint.clone(), key);
            key
        };
        let mut missed = false;
        let tile = atlas
            .get_or_insert(key, Size::new(width, height), || {
                missed = true;
                counter::bump(Counter::VectorMaskMisses);
                raster(&fingerprint)
            })
            .ok()?;
        if missed {
            budget.spend(texels);
            if volatile_cold {
                cold.spend(texels);
            }
        }
        history.track(part, key, superseded);
        history.route(part, Routed::Mask);
        Some(VectorMask { tile, key })
    }

    /// Whether `owner` may try the analytic route: false for [`HISTORY_FRAMES`] frames after it
    /// failed recognition.
    pub(crate) fn analytic_allowed(&self, owner: VectorId) -> bool {
        self.histories
            .get(&owner)
            .and_then(|history| history.analytic_declined)
            .is_none_or(|declined| self.frame.wrapping_sub(declined) >= HISTORY_FRAMES)
    }

    /// Records that a large shape of `owner` failed recognition in this frame.
    ///
    /// The entry counts as touched, so the sweep keeps it for as long as the decline counts.
    pub(crate) fn note_analytic_declined(&mut self, owner: VectorId) {
        let frame = self.frame;
        let history = self.histories.entry(owner).or_insert_with(|| RouteHistory {
            frame,
            ..RouteHistory::default()
        });
        history.advance(frame);
        history.analytic_declined = Some(frame);
    }

    /// Whether `owner` may try the marks route: false for [`HISTORY_FRAMES`] frames after it
    /// failed recognition there.
    pub(crate) fn marks_allowed(&self, owner: VectorId) -> bool {
        self.histories
            .get(&owner)
            .and_then(|history| history.marks_declined)
            .is_none_or(|declined| self.frame.wrapping_sub(declined) >= HISTORY_FRAMES)
    }

    /// Records that a large shape of `owner` failed recognition on the marks route in this
    /// frame.
    pub(crate) fn note_marks_declined(&mut self, owner: VectorId) {
        let frame = self.frame;
        let history = self.histories.entry(owner).or_insert_with(|| RouteHistory {
            frame,
            ..RouteHistory::default()
        });
        history.advance(frame);
        history.marks_declined = Some(frame);
    }

    /// How many geometry identities map to a tile.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Drops metadata whose atlas content was evicted.
    pub(crate) fn forget_tiles(&mut self, removed: &[AtlasKey]) {
        if removed.is_empty() {
            return;
        }
        self.entries.retain(|_, key| !removed.contains(key));
    }

    /// Forgets every geometry identity after the atlas itself is cleared.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.histories.clear();
        self.superseded.clear();
        self.recognitions.clear();
        self.payloads.clear();
        self.next_handle = MASK_NAMESPACE;
    }
}

/// A mask key no tile holds, from the mask namespace.
fn fresh_key(next_handle: &mut u64, atlas: &Atlas) -> AtlasKey {
    loop {
        let key = AtlasKey::new(*next_handle, TextureKind::Mono);
        *next_handle = MASK_NAMESPACE | (next_handle.wrapping_add(1) & HANDLE_BITS);
        if !atlas.contains(key) {
            return key;
        }
    }
}

/// The index of a request's part: 0 for the fill, 1 for the stroke.
fn part_of(style: VectorMaskStyle<'_>) -> usize {
    match style {
        VectorMaskStyle::Fill(_) => 0,
        VectorMaskStyle::Stroke(_) => 1,
    }
}

/// A cheap identity of one request, which moves when its geometry is a new allocation or a new
/// placement.
///
/// The address is the address of the path inside its shared allocation. A new revision of a canvas
/// allocates a new path, a new fit of a drawing is a new placement, and a replayed or unchanged one
/// keeps both. An address the allocator reuses hides at most one change. The fingerprint still
/// decides which tile is drawn, so the picture stays correct.
fn stamp(request: &VectorMaskRequest<'_>, part: usize) -> u64 {
    let mut hasher = FxHasher::default();
    core::ptr::from_ref(request.path).addr().hash(&mut hasher);
    request.path.elements().len().hash(&mut hasher);
    request.bounds.origin.x.hash(&mut hasher);
    request.bounds.origin.y.hash(&mut hasher);
    request.bounds.size.width.hash(&mut hasher);
    request.bounds.size.height.hash(&mut hasher);
    request.density[0].to_bits().hash(&mut hasher);
    request.density[1].to_bits().hash(&mut hasher);
    for coefficient in request.placement.as_coeffs() {
        coefficient.to_bits().hash(&mut hasher);
    }
    request.scale.to_bits().hash(&mut hasher);
    part.hash(&mut hasher);
    hasher.finish().max(1)
}

/// Whether a volatile shape is declined: always while the general rasteriser is built, and only
/// past [`COLD_SEGMENTS`] while it is cold.
fn declines(raster_ready: bool, path: &BezPath) -> bool {
    raster_ready || segments(path) > COLD_SEGMENTS
}

/// How many drawing segments a path has: its lines, quadratics and cubics.
fn segments(path: &BezPath) -> usize {
    path.elements()
        .iter()
        .filter(|element| {
            matches!(
                element,
                PathEl::LineTo(_) | PathEl::QuadTo(..) | PathEl::CurveTo(..)
            )
        })
        .count()
}

/// The path's commands in mask texels: placed, scaled by the density and moved to the origin.
fn commands(
    path: &BezPath,
    placement: zgui_scene::kurbo::Affine,
    density: [f32; 2],
    origin_x: i32,
    origin_y: i32,
) -> Option<Vec<Command>> {
    let x = f64::from(origin_x);
    let y = f64::from(origin_y);
    let kx = f64::from(density[0]);
    let ky = f64::from(density[1]);
    let placed = placement != zgui_scene::kurbo::Affine::IDENTITY;
    let point = |point: zgui_scene::kurbo::Point| {
        let point = if placed { placement * point } else { point };
        let point = [(point.x * kx - x) as f32, (point.y * ky - y) as f32];
        (point[0].is_finite() && point[1].is_finite())
            .then_some([point[0].to_bits(), point[1].to_bits()])
    };
    path.elements()
        .iter()
        .map(|element| match *element {
            PathEl::MoveTo(p) => Some(Command::Move(point(p)?)),
            PathEl::LineTo(p) => Some(Command::Line(point(p)?)),
            PathEl::QuadTo(a, b) => {
                let a = point(a)?;
                let b = point(b)?;
                Some(Command::Quad([a[0], a[1], b[0], b[1]]))
            }
            PathEl::CurveTo(a, b, c) => {
                let a = point(a)?;
                let b = point(b)?;
                let c = point(c)?;
                Some(Command::Curve([a[0], a[1], b[0], b[1], c[0], c[1]]))
            }
            PathEl::ClosePath => Some(Command::Close),
        })
        .collect()
}

fn raster(fingerprint: &Fingerprint) -> Vec<u8> {
    let commands: Vec<zeno::Command> = fingerprint
        .commands
        .iter()
        .map(|command| match *command {
            Command::Move(p) => zeno::Command::MoveTo(point(p)),
            Command::Line(p) => zeno::Command::LineTo(point(p)),
            Command::Quad(p) => zeno::Command::QuadTo(point([p[0], p[1]]), point([p[2], p[3]])),
            Command::Curve(p) => zeno::Command::CurveTo(
                point([p[0], p[1]]),
                point([p[2], p[3]]),
                point([p[4], p[5]]),
            ),
            Command::Close => zeno::Command::Close,
        })
        .collect();
    let mut mask = zeno::Mask::new(commands.as_slice());
    let dashes;
    match &fingerprint.style {
        Style::Fill(even_odd) => {
            mask.style(if *even_odd {
                zeno::Fill::EvenOdd
            } else {
                zeno::Fill::NonZero
            });
        }
        Style::Stroke {
            width,
            join,
            miter_limit,
            start_cap,
            end_cap,
            dashes: dash_bits,
            dash_offset,
        } => {
            dashes = dash_bits
                .iter()
                .map(|value| f32::from_bits(*value))
                .collect::<Vec<_>>();
            mask.style(zeno::Stroke {
                width: f32::from_bits(*width),
                join: zeno_join(*join),
                miter_limit: f32::from_bits(*miter_limit),
                start_cap: zeno_cap(*start_cap),
                end_cap: zeno_cap(*end_cap),
                dashes: &dashes,
                offset: f32::from_bits(*dash_offset),
                scale: true,
            });
        }
    }
    mask.size(
        fingerprint.size[0].max(0) as u32,
        fingerprint.size[1].max(0) as u32,
    );
    mask.render().0
}

/// Converts the public, borrowed style into exact bits for the cache and raster closure.
///
/// Every length of a stroke is measured along the outline, so `density` scales all of them. It is
/// one number rather than two because stroking does not commute with a map that scales the two axes
/// differently, and the caller declines the mask rather than ask this to draw the wrong outline.
/// The miter limit is a ratio and is left alone.
fn style(style: VectorMaskStyle<'_>, density: f32) -> Option<Style> {
    match style {
        VectorMaskStyle::Fill(rule) => Some(Style::Fill(rule == peniko::Fill::EvenOdd)),
        VectorMaskStyle::Stroke(stroke) => {
            let finite = [stroke.width, stroke.miter_limit, stroke.dash_offset]
                .into_iter()
                .chain(stroke.dash_pattern.iter().copied())
                .all(f64::is_finite);
            if !finite || stroke.width <= 0.0 {
                return None;
            }
            let scaled = |value: f64| ((value * f64::from(density)) as f32).to_bits();
            Some(Style::Stroke {
                width: scaled(stroke.width),
                join: kurbo_join(stroke.join),
                miter_limit: (stroke.miter_limit as f32).to_bits(),
                start_cap: kurbo_cap(stroke.start_cap),
                end_cap: kurbo_cap(stroke.end_cap),
                dashes: stroke
                    .dash_pattern
                    .iter()
                    .map(|value| scaled(*value))
                    .collect(),
                dash_offset: scaled(stroke.dash_offset),
            })
        }
    }
}

fn kurbo_join(join: kurbo::Join) -> u8 {
    match join {
        kurbo::Join::Bevel => 0,
        kurbo::Join::Miter => 1,
        kurbo::Join::Round => 2,
    }
}

fn zeno_join(join: u8) -> zeno::Join {
    match join {
        0 => zeno::Join::Bevel,
        2 => zeno::Join::Round,
        _ => zeno::Join::Miter,
    }
}

fn kurbo_cap(cap: kurbo::Cap) -> u8 {
    match cap {
        kurbo::Cap::Butt => 0,
        kurbo::Cap::Square => 1,
        kurbo::Cap::Round => 2,
    }
}

fn zeno_cap(cap: u8) -> zeno::Cap {
    match cap {
        1 => zeno::Cap::Square,
        2 => zeno::Cap::Round,
        _ => zeno::Cap::Butt,
    }
}

fn point(bits: [u32; 2]) -> zeno::Point {
    zeno::Point::new(f32::from_bits(bits[0]), f32::from_bits(bits[1]))
}

/// A stable diagnostic fingerprint for tests and cache instrumentation.
#[allow(dead_code)]
fn geometry_hash(fingerprint: &Fingerprint) -> u64 {
    let mut hasher = FxHasher::default();
    fingerprint.hash(&mut hasher);
    hasher.finish()
}
