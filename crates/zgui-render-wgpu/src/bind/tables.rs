//! The side tables, in the form a shader reads them.
//!
//! Clips, paints and transforms are addressed by index from every instance, so they travel in
//! storage buffers rather than in the instances themselves: an N-stop gradient then costs a quad
//! exactly as many instance bytes as a flat colour.

use bytemuck::{Pod, Zeroable};
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation, Interpolation};
use zgui_geom::Matrix4;
use zgui_profile::{Counter, counter};
use zgui_scene::{
    ChangeCoverage, ClipId, ClipLink, ClipNode, ClipTable, GradientKind, Paint, PaintId,
    PaintTable, Placements, ResolvedClip, Scene, SpatialTree, SpriteTile, TableVersion,
};

/// One rounded-corner test of a clip chain.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuRounded {
    /// The rectangle, as `[x, y, width, height]`.
    pub rect: [f32; 4],
    /// Elliptical radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
}

/// A whole clip chain, flattened into what one draw call applies.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuClip {
    /// The intersection of every rectangle in the chain.
    pub aabb: [f32; 4],
    /// The outermost rounded test.
    pub first: GpuRounded,
    /// The innermost rounded test.
    pub second: GpuRounded,
    /// How many of the two are meaningful.
    pub count: u32,
    /// One when the chain samples a coverage mask.
    pub has_mask: u32,
    /// The mask tile, meaningless unless `has_mask` is set.
    pub mask: SpriteTile,
}

/// One paint source.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuPaint {
    /// The family: nothing, solid, gradient or image.
    pub kind: u32,
    /// The gradient's shape: linear, radial or conic.
    pub gradient: u32,
    /// The space the ramp's stops are written in.
    pub space: u32,
    /// One when the ramp repeats outside its extent.
    pub flags: u32,
    /// The ramp's geometry, meaning whatever its shape says it does.
    pub geometry: [f32; 4],
    /// The colour of a solid paint, premultiplied and gamma-encoded.
    pub color: [f32; 4],
    /// Where this paint's stops begin.
    pub stop_start: u32,
    /// How many stops it has.
    pub stop_count: u32,
    /// Written zero, so the structure has no padding.
    pub pad0: u32,
    /// Written zero, so the structure has no padding.
    pub pad1: u32,
}

/// One stop of a ramp, in the space the ramp is interpolated in, premultiplied.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuStop {
    /// The three channels of the interpolation space, then the alpha.
    pub color: [f32; 4],
    /// Where along the ramp this stop sits.
    pub offset: f32,
    /// Written zero, so the structure has no padding.
    pub pad: [f32; 3],
}

/// One coordinate system: the matrix mapping it onto the device, column by column.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuSpatial {
    /// The matrix, as its four columns.
    pub matrix: [[f32; 4]; 4],
}

impl Default for GpuSpatial {
    fn default() -> Self {
        Self::of(Matrix4::IDENTITY)
    }
}

impl GpuSpatial {
    /// The shader's form of `matrix`.
    pub fn of(matrix: Matrix4) -> Self {
        Self {
            matrix: matrix.columns,
        }
    }
}

/// The family discriminants, which travel in the instance so a solid fill never reads the table.
pub mod kind {
    /// Paints nothing.
    pub const NONE: u32 = 0;
    /// One colour everywhere.
    pub const SOLID: u32 = 1;
    /// A ramp between stops.
    pub const GRADIENT: u32 = 2;
    /// A sampled image.
    pub const IMAGE: u32 = 3;
}

/// The interpolation spaces the shader can convert back from, and what it calls each.
pub mod space {
    /// Gamma-encoded sRGB, which needs no conversion at all.
    pub const SRGB: u32 = 0;
    /// Oklab.
    pub const OKLAB: u32 = 1;
    /// sRGB primaries with a linear transfer function.
    pub const LINEAR_SRGB: u32 = 2;
}

/// Every side table of one frame, flattened.
#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// The clip chains.
    pub clips: Vec<GpuClip>,
    /// The paint sources.
    pub paints: Vec<GpuPaint>,
    /// Every ramp's stops, concatenated.
    pub stops: Vec<GpuStop>,
    /// Every coordinate system, addressed by the slot a primitive names.
    pub spatial: Vec<GpuSpatial>,
}

impl Tables {
    /// Flattens `scene`'s side tables.
    pub fn of(scene: &Scene) -> Self {
        Self {
            clips: clips(&scene.clips, &scene.spatial),
            spatial: spatial(&scene.spatial),
            ..paints(&scene.paints)
        }
    }
}

/// Slots of one GPU table that need to be copied this frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirtySlots {
    /// Every slot is dirty. Used for the first frame, a journal miss, or a non-append paint edit.
    pub all: bool,
    /// Individual changed slots when a full copy is unnecessary.
    pub slots: Vec<u32>,
}

impl DirtySlots {
    /// Reuses the slot list for another frame.
    fn clear(&mut self) {
        self.all = false;
        self.slots.clear();
    }

    /// Marks the whole table dirty and discards narrower work.
    fn mark_all(&mut self) {
        self.all = true;
        self.slots.clear();
    }

    /// Sorts and deduplicates the changed slots before they become upload ranges.
    fn finish(&mut self) {
        if !self.all {
            self.slots.sort_unstable();
            self.slots.dedup();
        }
    }

    /// How many entries will be prepared or uploaded for a table of `total` slots.
    fn count(&self, total: usize) -> u64 {
        if self.all {
            total as u64
        } else {
            self.slots.len() as u64
        }
    }
}

/// The changes made while preparing all four side tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirtyTables {
    /// Flattened clip chains.
    pub clips: DirtySlots,
    /// Paint descriptors.
    pub paints: DirtySlots,
    /// Concatenated gradient stops.
    pub stops: DirtySlots,
    /// Resolved coordinate systems.
    pub spatial: DirtySlots,
}

impl DirtyTables {
    /// Reuses every dirty-slot list for another frame.
    fn clear(&mut self) {
        self.clips.clear();
        self.paints.clear();
        self.stops.clear();
        self.spatial.clear();
    }
}

/// Renderer-owned, reusable CPU copies of the shader side tables.
///
/// Scene tables keep stable slots and publish a bounded change journal. This cache consumes that
/// journal without draining it, so preparing a frame with unchanged side tables is allocation-free
/// and produces no side-table upload at all.
#[derive(Clone, Debug, Default)]
pub struct PreparedTables {
    tables: Tables,
    clips_version: TableVersion,
    paints_version: TableVersion,
    placements: Placements,
    dirty: DirtyTables,
    changed_clips: Vec<ClipId>,
    changed_paints: Vec<PaintId>,
    // Dense rather than hashed: probed per slot in the spatial write loop, where a hash per
    // membership test was most of the frame on a grown table.
    moved_spaces: Vec<bool>,
    /// How many space slots moved this frame, so an untouched frame skips the clip refresh.
    moved_space_count: usize,
    /// Which clip slots depend on which, so a frame refreshes the dependents of what changed
    /// rather than walking every chain in the id space.
    deps: ClipDependents,
}

/// The meta a slot holding no chain link is indexed under.
const NO_META: (u32, u32) = (u32::MAX, u32::MAX);

/// The (parent slot, own space slot) a clip table entry hangs from.
fn meta_of(node: Option<&ClipNode>) -> (u32, u32) {
    match node {
        Some(ClipNode::Link { link, parent, .. }) => {
            let space = match link {
                ClipLink::RoundedRect { space, .. } => space.index(),
                ClipLink::Mask { transform, .. } => transform.index(),
            };
            (parent.0, space)
        }
        _ => NO_META,
    }
}

/// The clip table's dependency edges, kept between frames.
///
/// A slot's resolved value depends on its own entry, its ancestors' entries, and the placement of
/// every space along the chain — so when one of those moves, the slot and everything applied
/// inside it refresh. Edges are only ever added: a slot reused for a different chain gains its
/// new edges and leaves the old ones standing as false candidates, which cost one wasted
/// comparison each and never a wrong pixel. The whole index is rebuilt when the stale share
/// passes a watermark, and whenever the consumer refreshes every slot anyway.
#[derive(Clone, Debug, Default)]
struct ClipDependents {
    /// The meta each clip slot was last indexed under.
    meta: Vec<(u32, u32)>,
    /// The clip slots applied directly inside each clip slot.
    children: Vec<Vec<u32>>,
    /// The clip slots whose own link is measured in each space slot.
    users: Vec<Vec<u32>>,
    /// How many memberships stand, for the staleness watermark.
    edges: usize,
    /// Which slots this frame has queued, cleared again when the frame settles.
    queued: Vec<bool>,
    /// The slots to refresh, in discovery order.
    stack: Vec<u32>,
    /// How far the refresh has read into the stack.
    cursor: usize,
}

impl ClipDependents {
    /// Grows the per-slot storage to cover `slots`.
    fn reach(&mut self, slots: usize) {
        if self.meta.len() < slots {
            self.meta.resize(slots, NO_META);
            self.children.resize_with(slots, Vec::new);
            self.queued.resize(slots, false);
        }
    }

    /// Whether the stale edges have accrued past what a rebuild costs to clear.
    fn wants_rebuild(&self, slots: usize) -> bool {
        self.edges > slots.saturating_mul(4) + 1_024
    }

    /// Rebuilds every edge from the table as it stands.
    fn rebuild(&mut self, table: &ClipTable) {
        for list in &mut self.children {
            list.clear();
        }
        for list in &mut self.users {
            list.clear();
        }
        self.meta.clear();
        self.edges = 0;
        self.reach(table.slots());
        for slot in 0..table.slots() as u32 {
            self.note(slot, meta_of(table.get(ClipId(slot))));
        }
    }

    /// Records where one slot hangs, adding its edges.
    fn note(&mut self, slot: u32, meta: (u32, u32)) {
        self.reach(slot as usize + 1);
        self.meta[slot as usize] = meta;
        let (parent, space) = meta;
        if parent != u32::MAX {
            self.reach(parent as usize + 1);
            self.children[parent as usize].push(slot);
            self.edges += 1;
        }
        if space != u32::MAX {
            if self.users.len() <= space as usize {
                self.users.resize_with(space as usize + 1, Vec::new);
            }
            self.users[space as usize].push(slot);
            self.edges += 1;
        }
    }

    /// Queues one slot for refresh, once.
    fn queue(&mut self, slot: u32) {
        self.reach(slot as usize + 1);
        if !self.queued[slot as usize] {
            self.queued[slot as usize] = true;
            self.stack.push(slot);
        }
    }

    /// Queues every recorded user of one space.
    fn queue_users(&mut self, space: u32) {
        let Some(users) = self.users.get(space as usize) else {
            return;
        };
        for index in 0..users.len() {
            let slot = self.users[space as usize][index];
            self.queue(slot);
        }
    }

    /// The next queued slot, with everything applied directly inside it queued behind it.
    fn next(&mut self) -> Option<u32> {
        let slot = *self.stack.get(self.cursor)?;
        self.cursor += 1;
        for index in 0..self.children[slot as usize].len() {
            let child = self.children[slot as usize][index];
            self.queue(child);
        }
        Some(slot)
    }

    /// Ends the frame's refresh, clearing the queue for the next one.
    fn settle(&mut self) {
        for slot in self.stack.drain(..) {
            self.queued[slot as usize] = false;
        }
        self.cursor = 0;
    }
}

impl PreparedTables {
    /// Updates the cached tables and records which slots changed.
    pub fn update(&mut self, scene: &Scene) {
        self.dirty.clear();
        self.changed_clips.clear();
        self.changed_paints.clear();
        self.moved_spaces.clear();
        self.moved_space_count = 0;

        let clip_coverage = scene
            .clips
            .changes_since(self.clips_version, &mut self.changed_clips);
        let paint_coverage = scene
            .paints
            .changes_since(self.paints_version, &mut self.changed_paints);

        self.update_spatial(&scene.spatial);
        self.update_paints(&scene.paints, paint_coverage);
        self.update_clips(&scene.clips, clip_coverage);

        self.clips_version = scene.clips.version();
        self.paints_version = scene.paints.version();
        self.dirty.clips.finish();
        self.dirty.paints.finish();
        self.dirty.stops.finish();
        self.dirty.spatial.finish();
        let prepared = self.dirty.clips.count(self.tables.clips.len())
            + self.dirty.paints.count(self.tables.paints.len())
            + self.dirty.stops.count(self.tables.stops.len())
            + self.dirty.spatial.count(self.tables.spatial.len());
        counter::add(Counter::SideTableSlotsPrepared, prepared);
    }

    /// The retained table storage, for memory reporting and tests.
    pub fn tables(&self) -> &Tables {
        &self.tables
    }

    /// The slots changed by the most recent [`PreparedTables::update`].
    pub fn dirty(&self) -> &DirtyTables {
        &self.dirty
    }

    fn update_spatial(&mut self, tree: &SpatialTree) {
        let first = self.placements.is_empty();
        self.placements.take_noting_slots(tree, &mut |slot, _| {
            self.dirty.spatial.slots.push(slot);
            note(&mut self.moved_spaces, slot as usize);
            self.moved_space_count += 1;
        });
        self.tables
            .spatial
            .resize(self.placements.len(), GpuSpatial::default());
        if first {
            self.dirty.spatial.mark_all();
        }
        if self.dirty.spatial.all {
            for (out, matrix) in self
                .tables
                .spatial
                .iter_mut()
                .zip(self.placements.matrices())
            {
                *out = GpuSpatial::of(matrix);
            }
        } else {
            for (slot, matrix) in self.placements.matrices().enumerate() {
                if noted(&self.moved_spaces, slot) {
                    self.tables.spatial[slot] = GpuSpatial::of(matrix);
                }
            }
        }
    }

    fn update_paints(&mut self, table: &PaintTable, coverage: ChangeCoverage) {
        if coverage == ChangeCoverage::All {
            let rebuilt = paints(table);
            self.tables.paints = rebuilt.paints;
            self.tables.stops = rebuilt.stops;
            self.dirty.paints.mark_all();
            self.dirty.stops.mark_all();
            return;
        }
        if self.changed_paints.is_empty() {
            return;
        }

        let old_paints = self.tables.paints.len();
        let append_only = table.slots() >= old_paints
            && self
                .changed_paints
                .iter()
                .all(|id| id.0 as usize >= old_paints);
        if !append_only {
            let rebuilt = paints(table);
            self.tables.paints = rebuilt.paints;
            self.tables.stops = rebuilt.stops;
            self.dirty.paints.mark_all();
            self.dirty.stops.mark_all();
            return;
        }

        let old_stops = self.tables.stops.len();
        for index in old_paints..table.slots() {
            push_paint(&mut self.tables, table.get(PaintId(index as u32)));
            self.dirty.paints.slots.push(index as u32);
        }
        self.dirty
            .stops
            .slots
            .extend((old_stops..self.tables.stops.len()).map(|index| index as u32));
    }

    fn update_clips(&mut self, table: &ClipTable, coverage: ChangeCoverage) {
        if coverage == ChangeCoverage::All {
            self.tables.clips = clips_with(table, &self.placements);
            self.dirty.clips.mark_all();
            self.deps.rebuild(table);
            return;
        }
        self.tables.clips.resize(table.slots(), freed_clip());
        self.deps.reach(table.slots());
        if self.changed_clips.is_empty()
            && self.moved_space_count == 0
            && !self.dirty.spatial.all
        {
            return;
        }
        if self.deps.wants_rebuild(table.slots()) {
            self.deps.rebuild(table);
        }
        // Seed with every journaled slot, re-homed in the index where its chain changed shape,
        // and with every recorded dependent of a space that moved. The closure below carries the
        // refresh down to everything applied inside a refreshed slot.
        for index in 0..self.changed_clips.len() {
            let id = self.changed_clips[index];
            let current = meta_of(table.get(id));
            if self.deps.meta[id.0 as usize] != current {
                self.deps.note(id.0, current);
            }
            self.deps.queue(id.0);
        }
        if self.dirty.spatial.all {
            for slot in 0..table.slots() as u32 {
                self.deps.queue(slot);
            }
        } else {
            for index in 0..self.dirty.spatial.slots.len() {
                let space = self.dirty.spatial.slots[index];
                self.deps.queue_users(space);
            }
        }
        while let Some(slot) = self.deps.next() {
            let id = ClipId(slot);
            let value = if table.contains(id) {
                gpu_clip(&table.resolve_placed(id, &|space| self.placements.get(space).copied()))
            } else {
                freed_clip()
            };
            // A false candidate — a stale edge, a chain a moved space no longer reaches —
            // resolves to what the slot already holds, and owes no upload.
            if self.tables.clips[slot as usize] != value {
                self.tables.clips[slot as usize] = value;
                self.dirty.clips.slots.push(slot);
            }
        }
        self.deps.settle();
    }
}

/// Marks one slot in a dense set, growing it to reach.
fn note(set: &mut Vec<bool>, slot: usize) {
    if slot >= set.len() {
        set.resize(slot + 1, false);
    }
    set[slot] = true;
}

/// Whether a dense set marks `slot`.
fn noted(set: &[bool], slot: usize) -> bool {
    set.get(slot).copied().unwrap_or(false)
}

/// Resolves clips through an already-computed placement cache.
fn clips_with(table: &ClipTable, placements: &Placements) -> Vec<GpuClip> {
    (0..table.slots())
        .map(|index| {
            let id = ClipId(index as u32);
            if table.contains(id) {
                gpu_clip(&table.resolve_placed(id, &|space| placements.get(space).copied()))
            } else {
                freed_clip()
            }
        })
        .collect()
}

/// Every clip chain, resolved into what a draw call binds.
///
/// One entry per slot rather than per live chain, for the reason [`spatial`] states: a primitive
/// names its chain by slot and the shader indexes this array with that number directly, so a count
/// of live chains would both truncate the array and shift every chain above a hole onto the wrong
/// entry.
///
/// Resolved through the frame's matrices, because the shader evaluates a clip at the device pixel
/// and a link interned inside a transformed subtree is measured in that subtree's own space — the
/// same matrices the content it clips is drawn through, read at the same moment.
fn clips(table: &ClipTable, spaces: &zgui_scene::SpatialTree) -> Vec<GpuClip> {
    (0..table.slots())
        .map(|index| {
            let id = zgui_scene::ClipId(index as u32);
            if table.contains(id) {
                gpu_clip(&table.resolve_placed(id, &|space| spaces.resolve(space)))
            } else {
                freed_clip()
            }
        })
        .collect()
}

/// What a slot no chain occupies holds.
///
/// An empty intersection rectangle, so a stale name clips its content away. Resolving the slot
/// instead would answer with the chain that clips *nothing*, because a chain nobody can reach has no
/// links — and a name that has outlived its chain would draw content across the whole surface with
/// nothing reporting it. Of the two ways to be wrong, the one that shows a missing box is the one
/// that can be seen.
fn freed_clip() -> GpuClip {
    GpuClip::default()
}

/// One resolved chain, in the shader's form.
fn gpu_clip(clip: &ResolvedClip) -> GpuClip {
    // A chain that samples a raster mask is applied where the mask is: in a target of its own.
    // Nothing binds one to a direct draw, and if anything ever does, this is where it is noticed
    // rather than where the mask is quietly ignored.
    debug_assert!(
        clip.mask.is_none(),
        "a raster mask cannot be applied by a direct draw"
    );
    GpuClip {
        aabb: clip.aabb,
        first: GpuRounded {
            rect: clip.rounded[0].rect,
            radii: clip.rounded[0].radii,
        },
        second: GpuRounded {
            rect: clip.rounded[1].rect,
            radii: clip.rounded[1].radii,
        },
        count: clip.rounded_count,
        has_mask: u32::from(clip.mask.is_some()),
        mask: clip.mask.map(SpriteTile::of).unwrap_or_default(),
    }
}

/// Every coordinate system, resolved, in the shader's form.
///
/// One entry per slot rather than per live node, because a primitive names its coordinate system by
/// slot and the shader indexes this array with that number directly. A slot nothing occupies holds
/// the identity, so a stale name reads as content drawn where it was laid out rather than as
/// whatever the slot's bytes happened to be.
fn spatial(tree: &zgui_scene::SpatialTree) -> Vec<GpuSpatial> {
    zgui_scene::Placements::of(tree)
        .matrices()
        .map(GpuSpatial::of)
        .collect()
}

/// Every paint source and its stops.
///
/// One entry per slot, for the same reason [`clips`] is: an instance carries the slot number, and a
/// slot nothing occupies paints nothing.
fn paints(table: &PaintTable) -> Tables {
    let mut tables = Tables::default();
    for index in 0..table.slots() {
        let id = zgui_scene::PaintId(index as u32);
        push_paint(&mut tables, table.get(id));
    }
    tables
}

/// Appends one paint and any stops it owns.
fn push_paint(tables: &mut Tables, paint: Option<&Paint>) {
    let Some(paint) = paint else {
        tables.paints.push(GpuPaint::default());
        return;
    };
    let gpu = match paint {
        Paint::Solid(color) => GpuPaint {
            kind: kind::SOLID,
            color: color.to_premultiplied_srgb(),
            ..GpuPaint::default()
        },
        Paint::Gradient {
            kind: shape,
            stops,
            space: interpolation_space,
            hue,
            repeating,
        } => {
            let start = tables.stops.len() as u32;
            let (encoding, written) = ramp(stops, *interpolation_space, *hue);
            tables.stops.extend(written);
            GpuPaint {
                kind: kind::GRADIENT,
                gradient: gradient_tag(shape),
                space: encoding,
                flags: u32::from(*repeating),
                geometry: gradient_geometry(shape),
                color: [0.0; 4],
                stop_start: start,
                stop_count: tables.stops.len() as u32 - start,
                pad0: 0,
                pad1: 0,
            }
        }
        Paint::Image { .. } => GpuPaint {
            kind: kind::IMAGE,
            ..GpuPaint::default()
        },
    };
    tables.paints.push(gpu);
}

/// Which shape a ramp follows.
fn gradient_tag(kind: &GradientKind) -> u32 {
    match kind {
        GradientKind::Linear { .. } => 0,
        GradientKind::Radial { .. } => 1,
        GradientKind::Conic { .. } => 2,
    }
}

/// A ramp's geometry, packed the way its shape reads it.
fn gradient_geometry(kind: &GradientKind) -> [f32; 4] {
    match kind {
        GradientKind::Linear { start, end } => [start.x.0, start.y.0, end.x.0, end.y.0],
        GradientKind::Radial {
            center,
            radius_x,
            radius_y,
        } => [center.x.0, center.y.0, *radius_x, *radius_y],
        GradientKind::Conic { center, from_angle } => [center.x.0, center.y.0, 0.0, *from_angle],
    }
}

/// A ramp's stops, and which space the shader must read them in.
///
/// Three spaces are walked in the shader, because each is a straight line in coordinates it can
/// convert back from with a handful of arithmetic. Every other space — the polar ones, the wide
/// gamuts, CIE Lab — is approximated here instead, by adding stops along the true curve until the
/// straight lines between them are within an eight-bit step of it. Two mechanisms, one rule: the
/// ramp is walked where walking it is exact, and approximated where it is not.
fn ramp(stops: &[GradientStop], space: ColorSpace, hue: HueInterpolation) -> (u32, Vec<GpuStop>) {
    let interpolation = Interpolation::new(space).with_hue(hue);
    match space {
        ColorSpace::Srgb => (space::SRGB, stops.iter().map(premultiplied_srgb).collect()),
        ColorSpace::Oklab => (
            space::OKLAB,
            stops
                .iter()
                .map(|stop| premultiplied_components(stop, ColorSpace::Oklab))
                .collect(),
        ),
        ColorSpace::SrgbLinear => (
            space::LINEAR_SRGB,
            stops
                .iter()
                .map(|stop| premultiplied_components(stop, ColorSpace::SrgbLinear))
                .collect(),
        ),
        _ => (
            space::SRGB,
            zgui_color::densify(stops, interpolation)
                .iter()
                .map(premultiplied_srgb)
                .collect(),
        ),
    }
}

/// One stop as premultiplied gamma-encoded sRGB.
fn premultiplied_srgb(stop: &GradientStop) -> GpuStop {
    GpuStop {
        color: stop.color.to_premultiplied_srgb(),
        offset: stop.offset,
        pad: [0.0; 3],
    }
}

/// One stop as premultiplied components of `space`.
///
/// The premultiplication is what CSS specifies for gradient interpolation, and it is why a ramp
/// running to `transparent` does not darken through its middle.
fn premultiplied_components(stop: &GradientStop, space: ColorSpace) -> GpuStop {
    let converted: Color = stop.color.to_space(space);
    let alpha = converted.alpha();
    let [first, second, third] = converted.components();
    GpuStop {
        color: [first * alpha, second * alpha, third * alpha, alpha],
        offset: stop.offset,
        pad: [0.0; 3],
    }
}

/// What a table with a freed slot uploads.
#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};
    use zgui_scene::{
        ClipId, ClipLink, ClipTable, OwnSpace, PaintTable, PropertyOwner, ResolvedClip,
    };

    use super::{PreparedTables, clips, freed_clip, gpu_clip, kind, paints};

    /// A device rectangle.
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    /// A clip table of three slots whose middle one has been freed, with the id of the survivor
    /// above the hole.
    fn clips_with_a_hole() -> (ClipTable, ClipId, ClipId) {
        let mut table = ClipTable::rooted();
        table.begin_frame();
        let doomed = table.only(ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
        table.begin_frame();
        let kept = table.only(ClipLink::rect(rect(4.0, 4.0, 2.0, 2.0)));
        table.begin_frame();
        assert_eq!(
            table.evict_least_recently_used(),
            1,
            "only the coldest goes"
        );
        assert!(
            !table.contains(doomed),
            "the hole is where the test wants it"
        );
        (table, doomed, kept)
    }

    #[test]
    fn a_table_with_a_hole_uploads_every_slot() {
        let (table, doomed, _) = clips_with_a_hole();
        let uploaded = clips(&table, &zgui_scene::SpatialTree::with_viewport());
        assert_eq!(uploaded.len(), table.slots(), "one entry per slot");
        assert_eq!(uploaded.len(), table.len() + 1, "and one slot is a hole");
        assert_eq!(uploaded[doomed.0 as usize], freed_clip());

        let mut paint_table = PaintTable::new();
        paint_table.begin_frame();
        let gone = paint_table.solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0));
        paint_table.begin_frame();
        paint_table.solid(zgui_color::Color::srgb(0.0, 1.0, 0.0, 1.0));
        paint_table.begin_frame();
        assert_eq!(paint_table.evict_least_recently_used(), 1);
        let painted = paints(&paint_table).paints;
        assert_eq!(painted.len(), paint_table.slots());
        assert_eq!(
            painted[gone.0 as usize].kind,
            kind::NONE,
            "a hole paints nothing"
        );
    }

    #[test]
    fn a_freed_clip_slot_does_not_read_as_unbounded() {
        let (table, doomed, _) = clips_with_a_hole();
        assert_eq!(
            table.resolve(doomed),
            ResolvedClip::unbounded(),
            "resolving a freed slot is the trap this guards"
        );
        let uploaded = clips(&table, &zgui_scene::SpatialTree::with_viewport());
        assert_ne!(
            uploaded[doomed.0 as usize],
            gpu_clip(&ResolvedClip::unbounded()),
            "a stale name must not draw across the whole surface"
        );
        assert_eq!(uploaded[doomed.0 as usize].aabb, [0.0; 4], "it clips away");
    }

    #[test]
    fn a_primitive_naming_a_slot_above_a_hole_resolves_to_its_own_entry() {
        let (table, doomed, kept) = clips_with_a_hole();
        assert!(kept.0 > doomed.0, "the survivor is above the hole");
        let uploaded = clips(&table, &zgui_scene::SpatialTree::with_viewport());
        assert_eq!(uploaded[kept.0 as usize], gpu_clip(&table.resolve(kept)));
        assert_eq!(uploaded[kept.0 as usize].aabb, [4.0, 4.0, 2.0, 2.0]);
    }

    #[test]
    fn an_unchanged_scene_prepares_no_side_table_slot_twice() {
        let scene = zgui_scene::Scene::new();
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);
        assert!(prepared.dirty().clips.all);
        assert!(prepared.dirty().spatial.all);

        prepared.update(&scene);
        let dirty = prepared.dirty();
        assert!(!dirty.clips.all && dirty.clips.slots.is_empty());
        assert!(!dirty.paints.all && dirty.paints.slots.is_empty());
        assert!(!dirty.stops.all && dirty.stops.slots.is_empty());
        assert!(!dirty.spatial.all && dirty.spatial.slots.is_empty());
    }

    #[test]
    fn an_appended_paint_dirties_only_its_new_slot() {
        let mut scene = zgui_scene::Scene::new();
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);

        let id = scene
            .paints
            .solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0));
        prepared.update(&scene);

        assert_eq!(prepared.dirty().paints.slots, vec![id.0]);
        assert!(!prepared.dirty().paints.all);
        assert_eq!(prepared.tables().paints[id.0 as usize].kind, kind::SOLID);
    }

    #[test]
    fn moving_a_space_dirties_its_matrix_and_dependent_clip() {
        let mut scene = zgui_scene::Scene::new();
        let owner = PropertyOwner::new(2).expect("a handle is never empty");
        let space = scene.spatial.space_of(
            scene.spatial.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::translation(4.0, 0.0, 0.0)), None, false),
        );
        let clip = scene.clips.only(ClipLink::RoundedRect {
            rect: rect(0.0, 0.0, 10.0, 10.0),
            radii: zgui_geom::Corners::default(),
            space,
        });
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);

        assert_eq!(
            scene.spatial.space_of(
                scene.spatial.viewport(),
                owner,
                OwnSpace::of(Some(Matrix4::translation(9.0, 0.0, 0.0)), None, false),
            ),
            space
        );
        prepared.update(&scene);

        assert_eq!(prepared.dirty().spatial.slots, vec![space.index()]);
        assert_eq!(prepared.dirty().clips.slots, vec![clip.0]);
        assert_eq!(prepared.tables().clips[clip.0 as usize].aabb[0], 9.0);
    }

    #[test]
    fn rewriting_a_parent_chain_refreshes_what_is_applied_inside_it() {
        let mut scene = zgui_scene::Scene::new();
        let owner = PropertyOwner::new(3).expect("a handle is never empty");
        let outer = scene.clips.push_named(
            ClipId::ROOT,
            ClipLink::rect(rect(0.0, 0.0, 100.0, 100.0)),
            Size::ZERO,
            owner,
        );
        let inner = scene
            .clips
            .push(outer, ClipLink::rect(rect(10.0, 10.0, 50.0, 50.0)));
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);

        // The box the outer chain is named after moves, which rewrites the slot in place.
        let again = scene.clips.push_named(
            ClipId::ROOT,
            ClipLink::rect(rect(20.0, 0.0, 100.0, 100.0)),
            Size::ZERO,
            owner,
        );
        assert_eq!(again, outer, "the same box is the same chain");
        prepared.update(&scene);

        let mut dirtied = prepared.dirty().clips.slots.clone();
        dirtied.sort_unstable();
        assert_eq!(
            dirtied,
            vec![outer.0, inner.0],
            "the rewritten chain and the one applied inside it, and nothing else"
        );
        assert_eq!(
            prepared.tables().clips[inner.0 as usize].aabb[0],
            20.0,
            "the inner chain re-resolved through the moved outer rectangle"
        );
    }

    #[test]
    fn a_slot_reused_for_a_different_chain_follows_its_new_space() {
        let mut scene = zgui_scene::Scene::new();
        let doomed = scene
            .clips
            .only(ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);

        // The slot is freed and taken by a chain measured in a coordinate system of its own.
        for _ in 0..3 {
            scene.clips.begin_frame();
        }
        assert_eq!(scene.clips.evict_least_recently_used(), 1);
        let owner = PropertyOwner::new(4).expect("a handle is never empty");
        let space = scene.spatial.space_of(
            scene.spatial.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::translation(2.0, 0.0, 0.0)), None, false),
        );
        let reused = scene.clips.only(ClipLink::RoundedRect {
            rect: rect(0.0, 0.0, 10.0, 10.0),
            radii: zgui_geom::Corners::default(),
            space,
        });
        assert_eq!(reused.0, doomed.0, "the hole is taken again");
        prepared.update(&scene);
        assert_eq!(prepared.tables().clips[reused.0 as usize].aabb[0], 2.0);

        // The new home's space moves; the refresh has to reach the re-homed slot.
        scene.spatial.space_of(
            scene.spatial.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::translation(7.0, 0.0, 0.0)), None, false),
        );
        prepared.update(&scene);
        assert_eq!(prepared.dirty().clips.slots, vec![reused.0]);
        assert_eq!(prepared.tables().clips[reused.0 as usize].aabb[0], 7.0);
    }
}
