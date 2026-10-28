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
    ChangeCoverage, ClipId, ClipLink, ClipNode, ClipTable, GradientKind, KeptClip, Paint, PaintId,
    PaintTable, Placements, ResolvedClip, Scene, SpatialId, SpatialTree, TableVersion,
};

/// One rounded-corner test of a clip chain.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuRounded {
    /// The rectangle, as `[x, y, width, height]`.
    pub rect: [f32; 4],
    /// Elliptical radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
    /// The superellipse exponent the corners are cut with; two is the ellipse.
    pub shape: f32,
}

/// A whole clip chain, flattened into what one draw call applies.
///
/// The links measured in one coordinate system that only translates can stay in that system's
/// coordinates. The shader then tests them at the device point moved back by the system's
/// translation, so a move of the system (a scroll of its container) changes no clip slot. The
/// coverage is the same as with every link on the device when f32 holds the translation exactly,
/// and can differ by one rounding step on an edge when it does not.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuClip {
    /// The intersection of every rectangle in the chain that is on the device.
    pub aabb: [f32; 4],
    /// The outermost rounded test.
    pub first: GpuRounded,
    /// The innermost rounded test.
    pub second: GpuRounded,
    /// How many of the two are meaningful.
    pub count: u32,
    /// The slot of the coordinate system the kept links are measured in, or [`NO_SPACE`].
    pub space: u32,
    /// The intersection of the kept links, in that system's coordinates.
    pub kept_aabb: [f32; 4],
    /// Bit 0 for `first`, bit 1 for `second`: set when the test is measured in `space`.
    pub kept_tests: u32,
    /// Written zero, so the structure has no padding.
    pub pad: u32,
}

/// What [`GpuClip::space`] holds when every link of the chain is on the device.
pub const NO_SPACE: u32 = u32::MAX;

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
    /// One colour per border side, held as four stops.
    pub const SIDES: u32 = 4;
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
    /// The space slots that moved this frame, with the name each one holds now.
    moved_names: Vec<(u32, Option<SpatialId>)>,
    /// Per space slot, the name it holds while that system only translates.
    ///
    /// A chain can keep its links of a system only while this holds. When it changes, the chains
    /// that keep that system resolve again.
    translating: Vec<Option<SpatialId>>,
    /// The space slots whose entry in `translating` changed this frame.
    unkept: Vec<u32>,
    /// Which clip slots depend on which, so a frame refreshes the dependents of what changed
    /// rather than walking every chain in the id space.
    deps: ClipDependents,
    /// The space slots one chain puts on the device, reused from chain to chain.
    chain_spaces: Vec<u32>,
    /// How many clip slots the last update resolved again, changed or not.
    refreshed: usize,
}

/// The tag a membership of [`ClipDependents::keepers`] carries in a slot's registered list.
const KEEPS: u32 = 1 << 31;

/// The parent slot a clip table entry hangs from, or `u32::MAX` for none.
fn parent_of(node: Option<&ClipNode>) -> u32 {
    match node {
        Some(ClipNode::Link { parent, .. }) => parent.0,
        _ => u32::MAX,
    }
}

/// The clip table's dependency edges, kept between frames.
///
/// A slot's resolved value depends on its own entry, its ancestors' entries, and the placement of
/// each space whose links it puts on the device. A space whose links it keeps changes the value
/// only when that space stops being a translation. So a slot is a member of the users of every
/// space it puts on the device, of the keepers of the space it keeps, and of its parent's
/// children. Each refresh records what it read.
///
/// A slot that no longer reads a space leaves its old membership standing until that space next
/// queues its members, which drops it. A stale parent edge stands as a false candidate, which
/// costs one wasted comparison and never a wrong pixel. The whole index is rebuilt when the stale
/// edges outnumber the live ones.
#[derive(Clone, Debug, Default)]
struct ClipDependents {
    /// The parent each clip slot was last indexed under.
    parents: Vec<u32>,
    /// The clip slots applied directly inside each clip slot.
    children: Vec<Vec<u32>>,
    /// The clip slots that put a link measured in each space slot on the device.
    users: Vec<Vec<u32>>,
    /// The clip slots that keep the links measured in each space slot in its coordinates.
    keepers: Vec<Vec<u32>>,
    /// Per clip slot, the space memberships its last refresh recorded, keepers tagged [`KEEPS`].
    registered: Vec<Vec<u32>>,
    /// How many memberships stand.
    edges: usize,
    /// How many of them no refresh records any more.
    stale: usize,
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
        if self.parents.len() < slots {
            self.parents.resize(slots, u32::MAX);
            self.children.resize_with(slots, Vec::new);
            self.registered.resize_with(slots, Vec::new);
            self.queued.resize(slots, false);
        }
    }

    /// Whether the stale memberships have accrued past what a rebuild costs to clear.
    fn wants_rebuild(&self) -> bool {
        self.stale > (self.edges - self.stale) + 1_024
    }

    /// Drops every edge, and indexes each slot under its parent again.
    ///
    /// The space memberships come back as the slots refresh, so the caller queues every slot.
    fn clear(&mut self, table: &ClipTable) {
        for list in self
            .children
            .iter_mut()
            .chain(&mut self.users)
            .chain(&mut self.keepers)
            .chain(&mut self.registered)
        {
            list.clear();
        }
        self.parents.clear();
        self.edges = 0;
        self.stale = 0;
        self.reach(table.slots());
        for slot in 0..table.slots() as u32 {
            self.note_parent(slot, parent_of(table.get(ClipId(slot))));
        }
    }

    /// Records which slot one slot hangs from, adding its edge when that changed.
    fn note_parent(&mut self, slot: u32, parent: u32) {
        self.reach(slot as usize + 1);
        let old = std::mem::replace(&mut self.parents[slot as usize], parent);
        if old == parent {
            return;
        }
        if old != u32::MAX {
            self.stale += 1;
        }
        if parent != u32::MAX {
            self.reach(parent as usize + 1);
            self.children[parent as usize].push(slot);
            self.edges += 1;
        }
    }

    /// Records the space memberships one refresh of `slot` read, and marks the rest stale.
    fn register(&mut self, slot: u32, memberships: &[u32]) {
        let mut held = std::mem::take(&mut self.registered[slot as usize]);
        if held.len() == memberships.len() && memberships.iter().all(|m| held.contains(m)) {
            self.registered[slot as usize] = held;
            return;
        }
        self.stale += held.iter().filter(|m| !memberships.contains(m)).count();
        for &membership in memberships {
            if held.contains(&membership) {
                continue;
            }
            let space = (membership & !KEEPS) as usize;
            let lists = if membership & KEEPS != 0 {
                &mut self.keepers
            } else {
                &mut self.users
            };
            if lists.len() <= space {
                lists.resize_with(space + 1, Vec::new);
            }
            lists[space].push(slot);
            self.edges += 1;
        }
        held.clear();
        held.extend_from_slice(memberships);
        self.registered[slot as usize] = held;
    }

    /// Queues one slot for refresh, once.
    fn queue(&mut self, slot: u32) {
        self.reach(slot as usize + 1);
        if !self.queued[slot as usize] {
            self.queued[slot as usize] = true;
            self.stack.push(slot);
        }
    }

    /// Queues every live member of one space's users, or of its keepers, and drops the stale ones.
    ///
    /// A slot whose last refresh did not record the membership cannot change when the space
    /// moves. Without the drop, a slot that changed from a user to a keeper of a scrolled space
    /// would refresh again on every later scroll.
    fn queue_members(&mut self, keepers: bool, space: u32) {
        let (lists, membership) = if keepers {
            (&mut self.keepers, space | KEEPS)
        } else {
            (&mut self.users, space)
        };
        let Some(members) = lists.get_mut(space as usize) else {
            return;
        };
        let (registered, queued, stack) = (&self.registered, &mut self.queued, &mut self.stack);
        let before = members.len();
        // A member was registered by a refresh, so `registered` and `queued` already reach it.
        members.retain(|&slot| {
            if !registered[slot as usize].contains(&membership) {
                return false;
            }
            let queued = &mut queued[slot as usize];
            if !*queued {
                *queued = true;
                stack.push(slot);
            }
            true
        });
        let dropped = before - members.len();
        self.stale -= dropped;
        self.edges -= dropped;
    }

    /// The next queued slot, with everything applied directly inside it queued behind it.
    fn next(&mut self) -> Option<u32> {
        let slot = self.next_alone()?;
        for index in 0..self.children[slot as usize].len() {
            let child = self.children[slot as usize][index];
            self.queue(child);
        }
        Some(slot)
    }

    /// The next queued slot, alone.
    ///
    /// For a slot queued by a space: what is applied inside it is a member of that space's lists
    /// in its own right where it reads the space.
    fn next_alone(&mut self) -> Option<u32> {
        let slot = *self.stack.get(self.cursor)?;
        self.cursor += 1;
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
        self.moved_names.clear();
        self.unkept.clear();
        self.refreshed = 0;

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
        self.placements.take_noting_slots(tree, &mut |slot, held| {
            self.dirty.spatial.slots.push(slot);
            note(&mut self.moved_spaces, slot as usize);
            self.moved_names.push((slot, held));
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
        for &(slot, held) in &self.moved_names {
            let now = held.filter(|name| {
                self.placements
                    .get(*name)
                    .is_some_and(|matrix| matrix.plane_translation().is_some())
            });
            if self.translating.len() <= slot as usize {
                self.translating.resize(slot as usize + 1, None);
            }
            if self.translating[slot as usize] != now {
                self.translating[slot as usize] = now;
                self.unkept.push(slot);
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
        let in_place = table.slots() >= old_paints
            && self
                .changed_paints
                .iter()
                .all(|id| id.0 as usize >= old_paints || self.rewrites_in_place(table, *id));
        if !in_place {
            let rebuilt = paints(table);
            self.tables.paints = rebuilt.paints;
            self.tables.stops = rebuilt.stops;
            self.dirty.paints.mark_all();
            self.dirty.stops.mark_all();
            return;
        }

        for index in 0..self.changed_paints.len() {
            let id = self.changed_paints[index];
            if (id.0 as usize) < old_paints {
                self.rewrite_paint(table, id);
            }
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

    /// Whether the slot `id` can take its new paint where it stands.
    ///
    /// True for a flat colour that replaced a flat colour, and for four side colours that replaced
    /// four: neither moves another slot's stops. An owned paint an animation rewrites is one of
    /// the two, so a frame of a colour animation copies the slots it changed and nothing else.
    fn rewrites_in_place(&self, table: &PaintTable, id: PaintId) -> bool {
        let held = &self.tables.paints[id.0 as usize];
        match table.get(id) {
            Some(Paint::Solid(_)) => held.kind == kind::SOLID,
            Some(Paint::Sides(_)) => {
                held.kind == kind::SIDES
                    && held.stop_count == 4
                    && (held.stop_start as usize) + 4 <= self.tables.stops.len()
            }
            _ => false,
        }
    }

    /// Writes the new paint of a slot [`PreparedTables::rewrites_in_place`] admitted.
    fn rewrite_paint(&mut self, table: &PaintTable, id: PaintId) {
        let slot = id.0 as usize;
        match table.get(id) {
            Some(Paint::Solid(color)) => {
                self.tables.paints[slot].color = color.to_premultiplied_srgb();
                self.dirty.paints.slots.push(id.0);
            }
            Some(Paint::Sides(colors)) => {
                let start = self.tables.paints[slot].stop_start as usize;
                for (side, color) in colors.iter().enumerate() {
                    self.tables.stops[start + side].color = color.to_premultiplied_srgb();
                    self.dirty.stops.slots.push((start + side) as u32);
                }
            }
            _ => {}
        }
    }

    fn update_clips(&mut self, table: &ClipTable, coverage: ChangeCoverage) {
        self.tables.clips.resize(table.slots(), freed_clip());
        self.deps.reach(table.slots());
        let all = coverage == ChangeCoverage::All;
        if !all
            && self.changed_clips.is_empty()
            && self.moved_space_count == 0
            && !self.dirty.spatial.all
        {
            return;
        }
        if all || self.dirty.spatial.all || self.deps.wants_rebuild() {
            // Every slot refreshes, and records its edges again as it does.
            self.deps.clear(table);
            for slot in 0..table.slots() as u32 {
                self.deps.queue(slot);
            }
            self.refresh(table, false);
            if all {
                self.dirty.clips.mark_all();
            }
            self.deps.settle();
            return;
        }
        // Every journaled slot, re-homed in the index where its chain changed shape, and what is
        // applied inside it.
        for index in 0..self.changed_clips.len() {
            let id = self.changed_clips[index];
            // The sweep retracts the id space where its tail is free, and journals the slots it
            // frees — so a journaled slot can lie past the current end, where there is no entry
            // to write and nothing left to name it.
            if id.0 as usize >= table.slots() {
                continue;
            }
            self.deps.note_parent(id.0, parent_of(table.get(id)));
            self.deps.queue(id.0);
        }
        self.refresh(table, true);
        // Every slot that puts a moved space on the device, and every slot that keeps a space
        // that stopped translating or changed hands. A slot that keeps a space which still
        // translates reads where it is in the shader.
        for index in 0..self.dirty.spatial.slots.len() {
            let space = self.dirty.spatial.slots[index];
            self.deps.queue_members(false, space);
        }
        for index in 0..self.unkept.len() {
            let space = self.unkept[index];
            self.deps.queue_members(true, space);
        }
        self.refresh(table, false);
        self.deps.settle();
    }

    /// Resolves the queued slots again, and records each one whose value changed.
    ///
    /// `closure` queues what is applied inside each slot behind it.
    fn refresh(&mut self, table: &ClipTable, closure: bool) {
        loop {
            let next = if closure {
                self.deps.next()
            } else {
                self.deps.next_alone()
            };
            let Some(slot) = next else {
                break;
            };
            // A stale edge can also reach past a retracted id space.
            if slot as usize >= self.tables.clips.len() {
                continue;
            }
            self.refreshed += 1;
            let value = resolve_slot(
                table,
                ClipId(slot),
                &self.placements,
                &mut self.deps,
                &mut self.chain_spaces,
            );
            // A false candidate — a stale edge, a chain a moved space no longer reaches —
            // resolves to what the slot already holds, and owes no upload.
            if self.tables.clips[slot as usize] != value {
                self.tables.clips[slot as usize] = value;
                self.dirty.clips.slots.push(slot);
            }
        }
    }
}

/// One clip slot in the shader's form, with the space memberships its value reads registered.
///
/// The links measured in the chain's innermost coordinate system stay in that system's
/// coordinates when the system translates and is not the identity, which is what a scrolled
/// content space is. Every other link, and every link of a chain that cannot keep a part, is put
/// on the device through `placements`.
fn resolve_slot(
    table: &ClipTable,
    id: ClipId,
    placements: &Placements,
    deps: &mut ClipDependents,
    spaces: &mut Vec<u32>,
) -> GpuClip {
    spaces.clear();
    if !table.contains(id) {
        deps.register(id.0, spaces);
        return freed_clip();
    }
    let matrix_of = |space: SpatialId| placements.get(space).copied();
    let kept = innermost_space(table, id)
        .filter(|space| {
            placements
                .moves(*space)
                .is_some_and(|matrix| matrix.plane_translation().is_some())
        })
        .and_then(|space| Some((space, table.resolve_kept(id, &matrix_of, space)?)));
    let kept_name = kept.map(|(space, _)| space);
    let mut cursor = id;
    while let Some(ClipNode::Link { link, parent, .. }) = table.get(cursor) {
        if let ClipLink::RoundedRect { space, .. } = link
            && Some(*space) != kept_name
            && !spaces.contains(&space.index())
        {
            spaces.push(space.index());
        }
        cursor = *parent;
    }
    let value = match kept {
        Some((space, clip)) => {
            spaces.push(space.index() | KEEPS);
            gpu_kept(&clip, space.index())
        }
        None => gpu_clip(&table.resolve_placed(id, &matrix_of)),
    };
    deps.register(id.0, spaces);
    value
}

/// The coordinate system of the innermost rectangle `id` applies.
fn innermost_space(table: &ClipTable, id: ClipId) -> Option<SpatialId> {
    let mut cursor = id;
    while let Some(ClipNode::Link { link, parent, .. }) = table.get(cursor) {
        if let ClipLink::RoundedRect { space, .. } = link {
            return Some(*space);
        }
        cursor = *parent;
    }
    None
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
    GpuClip {
        space: NO_SPACE,
        ..GpuClip::default()
    }
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
            shape: clip.rounded[0].shape,
        },
        second: GpuRounded {
            rect: clip.rounded[1].rect,
            radii: clip.rounded[1].radii,
            shape: clip.rounded[1].shape,
        },
        count: clip.rounded_count,
        space: NO_SPACE,
        kept_aabb: ResolvedClip::unbounded().aabb,
        kept_tests: 0,
        pad: 0,
    }
}

/// One chain resolved with the links of the system at `slot` kept, in the shader's form.
fn gpu_kept(kept: &KeptClip, slot: u32) -> GpuClip {
    GpuClip {
        space: slot,
        kept_aabb: kept.kept_aabb,
        kept_tests: kept.kept_tests,
        ..gpu_clip(&kept.clip)
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
        Paint::Sides(colors) => {
            let start = tables.stops.len() as u32;
            tables.stops.extend(colors.iter().map(|color| GpuStop {
                color: color.to_premultiplied_srgb(),
                offset: 0.0,
                pad: [0.0; 3],
            }));
            GpuPaint {
                kind: kind::SIDES,
                stop_start: start,
                stop_count: 4,
                ..GpuPaint::default()
            }
        }
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
    use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size, Vec2};
    use zgui_scene::{
        ClipId, ClipLink, ClipTable, OwnSpace, PaintTable, PropertyOwner, ResolvedClip,
    };

    use super::{
        GpuClip, GpuRounded, GpuSpatial, NO_SPACE, PreparedTables, clips, freed_clip, gpu_clip,
        kind, paints,
    };

    /// The shader's `rect_coverage` for round corners, in Rust.
    fn rect_coverage(point: (f32, f32), test: &GpuRounded) -> f32 {
        let [x, y, w, h] = test.rect;
        let half = (w * 0.5, h * 0.5);
        let to_point = (point.0 - (x + half.0), point.1 - (y + half.1));
        let r = &test.radii;
        let (rx, ry) = match (to_point.0 < 0.0, to_point.1 < 0.0) {
            (true, true) => (r[0], r[1]),
            (true, false) => (r[6], r[7]),
            (false, true) => (r[2], r[3]),
            (false, false) => (r[4], r[5]),
        };
        assert_eq!(rx, ry, "the round corners the tests use");
        let corner = (
            to_point.0.abs() - half.0 + rx,
            to_point.1.abs() - half.1 + ry,
        );
        let sdf = if corner.0 > 0.0 && corner.1 > 0.0 && rx > 0.0 {
            (corner.0 * corner.0 + corner.1 * corner.1).sqrt() - rx
        } else {
            (corner.0 - rx).max(corner.1 - ry)
        };
        (0.5 - sdf).clamp(0.0, 1.0)
    }

    /// Whether a point lies outside an upright box, as the shader asks it.
    fn outside(point: (f32, f32), b: [f32; 4]) -> bool {
        point.0 < b[0] || point.1 < b[1] || point.0 > b[0] + b[2] || point.1 > b[1] + b[3]
    }

    /// The shader's `clip_coverage`, in Rust.
    fn clip_coverage(clip: &GpuClip, spatial: &[GpuSpatial], point: (f32, f32)) -> f32 {
        if outside(point, clip.aabb) {
            return 0.0;
        }
        let mut kept = point;
        if clip.space != NO_SPACE {
            let [x, y, ..] = spatial[clip.space as usize].matrix[3];
            kept = (point.0 - x, point.1 - y);
            if outside(kept, clip.kept_aabb) {
                return 0.0;
            }
        }
        let mut coverage = 1.0;
        for (index, test) in [&clip.first, &clip.second].into_iter().enumerate() {
            if clip.count > index as u32 {
                let at = if clip.kept_tests & (1 << index) != 0 {
                    kept
                } else {
                    point
                };
                coverage *= rect_coverage(at, test);
            }
        }
        coverage
    }

    /// Checks that every prepared clip slot covers each sampled pixel as the chain resolved onto
    /// the device does.
    ///
    /// Equal to within float rounding: the kept form moves the point and the device form moves
    /// the rectangle.
    fn assert_same_coverage(scene: &zgui_scene::Scene, prepared: &PreparedTables) {
        let tables = prepared.tables();
        let reference = clips(&scene.clips, &scene.spatial);
        assert_eq!(tables.clips.len(), reference.len());
        for (slot, (clip, device)) in tables.clips.iter().zip(&reference).enumerate() {
            for row in 0..60 {
                for column in 0..60 {
                    let point = (column as f32 * 1.7 + 0.5, row as f32 * 1.7 + 0.5);
                    let kept = clip_coverage(clip, &tables.spatial, point);
                    let placed = clip_coverage(device, &tables.spatial, point);
                    assert!(
                        (kept - placed).abs() <= 1.0e-4,
                        "slot {slot} at {point:?}: kept {kept}, on the device {placed}"
                    );
                }
            }
        }
    }

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
    fn a_rewritten_owned_paint_dirties_only_its_own_slot() {
        use zgui_scene::{Paint, PaintOwner, PaintRole};
        let mut scene = zgui_scene::Scene::new();
        let owner = PropertyOwner::new(7).expect("a handle is never empty");
        let fill = PaintOwner::new(owner, PaintRole::Background);
        let stroke = PaintOwner::new(owner, PaintRole::BorderSides);
        let red = zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0);
        let grey = zgui_color::Color::srgb(0.5, 0.5, 0.5, 1.0);
        scene.paints.solid(red);
        let solid = scene.paints.owned(fill, Paint::Solid(red));
        let sides = scene.paints.owned(stroke, Paint::Sides([red; 4]));
        scene.paints.solid(grey);
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);
        prepared.update(&scene);

        scene.paints.owned(fill, Paint::Solid(grey));
        scene
            .paints
            .owned(stroke, Paint::Sides([red, grey, red, grey]));
        prepared.update(&scene);
        let dirty = prepared.dirty();
        assert!(
            !dirty.paints.all && !dirty.stops.all,
            "the table was rebuilt"
        );
        assert_eq!(dirty.paints.slots, vec![solid.index]);
        assert_eq!(dirty.stops.slots.len(), 4);
        assert_eq!(
            prepared.tables().paints,
            paints(&scene.paints).paints,
            "the slots written in place hold what a rebuild holds"
        );
        assert_eq!(prepared.tables().stops, paints(&scene.paints).stops);
        assert_eq!(
            prepared.tables().paints[sides.index as usize].kind,
            kind::SIDES
        );
    }

    #[test]
    fn a_paint_sweep_that_frees_and_reuses_slots_is_absorbed() {
        use zgui_scene::{Paint, PaintOwner, PaintRole};
        let mut scene = zgui_scene::Scene::new();
        let red = zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0);
        let grey = zgui_color::Color::srgb(0.5, 0.5, 0.5, 1.0);
        let kept = scene.paints.solid(red);
        let gone = PaintOwner::new(
            PropertyOwner::new(7).expect("a handle is never empty"),
            PaintRole::BorderSides,
        );
        let middle = scene
            .paints
            .owned(gone, Paint::Sides([red, grey, red, grey]));
        let tail = scene.paints.solid(grey);
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);

        // The kept paint stays in use while the other two age out; the sweep frees both and
        // retracts the id space over the tail.
        for _ in 0..12 {
            scene.paints.begin_frame();
            scene.paints.solid(red);
        }
        assert_eq!(scene.paints.evict_unreachable(8, usize::MAX), 2);
        assert!(scene.paints.slots() <= tail.0 as usize);
        prepared.update(&scene);
        assert_eq!(prepared.tables().paints, paints(&scene.paints).paints);
        assert_eq!(prepared.tables().stops, paints(&scene.paints).stops);

        // A freed slot is taken by a paint of another family.
        let reused = scene.paints.solid(grey);
        assert_eq!(reused.0, middle.index, "the freed slot is reused");
        prepared.update(&scene);
        assert_eq!(prepared.tables().paints, paints(&scene.paints).paints);
        assert_eq!(prepared.tables().stops, paints(&scene.paints).stops);
        assert_eq!(prepared.tables().paints[kept.0 as usize].kind, kind::SOLID);
    }

    #[test]
    fn moving_a_space_dirties_its_matrix_and_dependent_clip() {
        let mut scene = zgui_scene::Scene::new();
        let owner = PropertyOwner::new(2).expect("a handle is never empty");
        let space = scene.spatial.space_of(
            scene.spatial.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::scale(2.0, 1.0, 1.0)), None, false),
        );
        let clip = scene.clips.only(ClipLink::RoundedRect {
            shape: zgui_scene::CornerShape::ROUND,
            rect: rect(1.0, 0.0, 10.0, 10.0),
            radii: zgui_geom::Corners::default(),
            space,
        });
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);
        assert_eq!(prepared.tables().clips[clip.0 as usize].space, NO_SPACE);

        assert_eq!(
            scene.spatial.space_of(
                scene.spatial.viewport(),
                owner,
                OwnSpace::of(Some(Matrix4::scale(3.0, 1.0, 1.0)), None, false),
            ),
            space
        );
        prepared.update(&scene);

        assert_eq!(prepared.dirty().spatial.slots, vec![space.index()]);
        assert_eq!(prepared.dirty().clips.slots, vec![clip.0]);
        assert_eq!(prepared.tables().clips[clip.0 as usize].aabb[0], 3.0);
    }

    #[test]
    fn a_translated_space_moves_without_touching_the_clips_that_keep_it() {
        let mut scene = zgui_scene::Scene::new();
        let owner = PropertyOwner::new(2).expect("a handle is never empty");
        let content = scene.spatial.space_of(
            scene.spatial.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::translation(0.0, -40.0, 0.0)), None, false),
        );
        let port = scene
            .clips
            .only(ClipLink::rect(rect(0.0, 0.0, 100.0, 100.0)));
        let row = scene.clips.push(
            port,
            ClipLink::rect_in(rect(0.0, 50.0, 80.0, 20.0), content),
        );
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);
        let kept = prepared.tables().clips[row.0 as usize];
        assert_eq!(
            kept.space,
            content.index(),
            "the row keeps its content space"
        );
        assert_eq!(kept.kept_aabb, [0.0, 50.0, 80.0, 20.0]);
        assert_eq!(
            kept.aabb,
            [0.0, 0.0, 100.0, 100.0],
            "the port stays on the device"
        );
        assert_eq!(prepared.tables().clips[port.0 as usize].space, NO_SPACE);

        for offset in [-75.0, -12.0, 0.0, -3.5] {
            scene
                .spatial
                .place(content, Matrix4::translation(0.0, offset, 0.0));
            prepared.update(&scene);
            assert_eq!(prepared.dirty().spatial.slots, vec![content.index()]);
            assert!(
                prepared.dirty().clips.slots.is_empty(),
                "a scroll of {offset} rewrites no clip slot"
            );
            assert_same_coverage(&scene, &prepared);
        }
    }

    #[test]
    fn a_space_born_at_the_identity_refreshes_its_rows_once_when_it_first_translates() {
        let mut scene = zgui_scene::Scene::new();
        let owner = PropertyOwner::new(2).expect("a handle is never empty");
        let content = scene.spatial.space_of(
            scene.spatial.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::IDENTITY), None, false),
        );
        let port = scene
            .clips
            .only(ClipLink::rect(rect(0.0, 0.0, 100.0, 100.0)));
        let rows: Vec<ClipId> = (0..1_000)
            .map(|row| {
                scene.clips.push(
                    port,
                    ClipLink::rect_in(rect(0.0, row as f32 * 20.0, 80.0, 20.0), content),
                )
            })
            .collect();
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);
        assert_eq!(
            prepared.tables().clips[rows[0].0 as usize].space,
            NO_SPACE,
            "a row at the identity is put on the device"
        );

        // The first move turns every row into a keeper of the space.
        scene
            .spatial
            .place(content, Matrix4::translation(0.0, -40.0, 0.0));
        prepared.update(&scene);
        assert!(prepared.refreshed >= rows.len());
        assert_eq!(
            prepared.tables().clips[rows[0].0 as usize].space,
            content.index()
        );
        assert_same_coverage(&scene, &prepared);

        // The rows left their user memberships stale, and a later move refreshes none of them.
        for offset in [-55.0, -70.0, -12.5] {
            scene
                .spatial
                .place(content, Matrix4::translation(0.0, offset, 0.0));
            prepared.update(&scene);
            assert!(prepared.dirty().clips.slots.is_empty());
            assert_eq!(
                prepared.refreshed, 0,
                "a scroll of {offset} resolves no clip slot again"
            );
            assert_same_coverage(&scene, &prepared);
        }
        assert!(prepared.deps.stale < 1_024);
    }

    #[test]
    fn a_kept_space_that_stops_translating_puts_its_links_on_the_device() {
        let mut scene = zgui_scene::Scene::new();
        let owner = PropertyOwner::new(2).expect("a handle is never empty");
        let content = scene.spatial.space_of(
            scene.spatial.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::translation(5.0, 0.0, 0.0)), None, false),
        );
        let clip = scene
            .clips
            .only(ClipLink::rect_in(rect(1.0, 0.0, 10.0, 10.0), content));
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);
        assert_eq!(
            prepared.tables().clips[clip.0 as usize].space,
            content.index()
        );

        scene.spatial.place(content, Matrix4::scale(2.0, 1.0, 1.0));
        prepared.update(&scene);
        assert_eq!(prepared.dirty().clips.slots, vec![clip.0]);
        let placed = prepared.tables().clips[clip.0 as usize];
        assert_eq!(placed.space, NO_SPACE);
        assert_eq!(placed.aabb[0], 2.0);
        assert_same_coverage(&scene, &prepared);

        // A move while on the device refreshes it, and a translation lets it keep the space again.
        scene
            .spatial
            .place(content, Matrix4::translation(9.0, 0.0, 0.0));
        prepared.update(&scene);
        assert_eq!(prepared.dirty().clips.slots, vec![clip.0]);
        assert_eq!(
            prepared.tables().clips[clip.0 as usize].space,
            content.index()
        );
        assert_same_coverage(&scene, &prepared);
    }

    #[test]
    fn the_index_rebuilt_after_many_flips_still_follows_the_kept_space() {
        let mut scene = zgui_scene::Scene::new();
        let owner = PropertyOwner::new(2).expect("a handle is never empty");
        let content = scene.spatial.space_of(
            scene.spatial.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::translation(0.0, -5.0, 0.0)), None, false),
        );
        let clip = scene
            .clips
            .only(ClipLink::rect_in(rect(1.0, 10.0, 30.0, 20.0), content));
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);

        // Each flip between the kept and the device form leaves one stale membership, so this
        // passes the rebuild watermark.
        for flip in 0..1_100 {
            let matrix = if flip % 2 == 0 {
                Matrix4::scale(2.0, 1.0, 1.0)
            } else {
                Matrix4::translation(0.0, -(flip as f32), 0.0)
            };
            scene.spatial.place(content, matrix);
            prepared.update(&scene);
            assert_eq!(prepared.dirty().clips.slots, vec![clip.0], "flip {flip}");
        }
        assert!(
            prepared.deps.stale < 1_024,
            "the watermark rebuilt the index"
        );

        scene
            .spatial
            .place(content, Matrix4::translation(0.0, -7.0, 0.0));
        prepared.update(&scene);
        assert!(prepared.dirty().clips.slots.is_empty());
        assert_same_coverage(&scene, &prepared);
        scene.spatial.place(content, Matrix4::scale(3.0, 1.0, 1.0));
        prepared.update(&scene);
        assert_eq!(prepared.dirty().clips.slots, vec![clip.0]);
        assert_same_coverage(&scene, &prepared);
    }

    #[test]
    fn a_kept_space_slot_taken_by_another_translating_space_puts_its_links_on_the_device() {
        let mut scene = zgui_scene::Scene::new();
        let viewport = scene.spatial.viewport();
        let first = PropertyOwner::new(2).expect("a handle is never empty");
        let content = scene.spatial.space_of(
            viewport,
            first,
            OwnSpace::of(Some(Matrix4::translation(0.0, -20.0, 0.0)), None, false),
        );
        let clip = scene
            .clips
            .only(ClipLink::rect_in(rect(1.0, 30.0, 40.0, 20.0), content));
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);
        assert_eq!(
            prepared.tables().clips[clip.0 as usize].space,
            content.index()
        );

        // The space is released, and in the same frame gap another box that only translates
        // takes its slot. The chain still names the released space.
        assert_eq!(scene.spatial.release(first), Some(content));
        scene.spatial.recycle();
        let other = scene.spatial.space_of(
            viewport,
            PropertyOwner::new(3).expect("a handle is never empty"),
            OwnSpace::of(Some(Matrix4::translation(0.0, -55.0, 0.0)), None, false),
        );
        assert_eq!(other.index(), content.index(), "the slot is taken again");
        assert_ne!(other, content);
        prepared.update(&scene);
        assert_eq!(prepared.dirty().clips.slots, vec![clip.0]);
        assert_eq!(
            prepared.tables().clips[clip.0 as usize].space,
            NO_SPACE,
            "the chain stops reading the new occupant's translation"
        );
        assert_same_coverage(&scene, &prepared);
    }

    #[test]
    fn a_kept_clip_draws_what_the_device_resolution_draws() {
        let mut scene = zgui_scene::Scene::new();
        let viewport = scene.spatial.viewport();
        let outer = scene.spatial.space_of(
            viewport,
            PropertyOwner::new(2).expect("a handle is never empty"),
            OwnSpace::of(Some(Matrix4::translation(0.0, -30.0, 0.0)), None, false),
        );
        let inner = scene.spatial.space_of(
            outer,
            PropertyOwner::new(3).expect("a handle is never empty"),
            OwnSpace::of(Some(Matrix4::translation(-7.0, -18.0, 0.0)), None, false),
        );
        let round = |r: f32| zgui_geom::Corners::uniform(Vec2::splat(DevicePx(r)));
        let shaped = |x, y, w, h, r, space| {
            ClipLink::shaped(
                rect(x, y, w, h),
                round(r),
                zgui_scene::CornerShape::ROUND,
                space,
            )
        };
        // A port, a rounded row in the outer content, a nested port and a rounded cell in the
        // inner content; then a third rounded test, which no slot can apply with a kept part.
        let port = scene.clips.only(ClipLink::rect(rect(4.0, 4.0, 90.0, 90.0)));
        let row = scene
            .clips
            .push(port, shaped(2.0, 30.0, 70.0, 40.0, 6.0, outer));
        let nested = scene
            .clips
            .push(row, ClipLink::rect_in(rect(10.0, 40.0, 50.0, 25.0), outer));
        let cell = scene
            .clips
            .push(nested, shaped(12.0, 50.0, 30.0, 30.0, 5.0, inner));
        let deep = scene
            .clips
            .push(cell, shaped(14.0, 52.0, 20.0, 20.0, 3.0, inner));
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);
        let tables = prepared.tables();
        assert_eq!(tables.clips[port.0 as usize].space, NO_SPACE);
        assert_eq!(tables.clips[row.0 as usize].space, outer.index());
        assert_eq!(tables.clips[row.0 as usize].kept_tests, 0b1);
        assert_eq!(tables.clips[nested.0 as usize].space, outer.index());
        assert_eq!(tables.clips[cell.0 as usize].space, inner.index());
        assert_eq!(
            tables.clips[cell.0 as usize].kept_tests, 0b10,
            "the cell's own test is the inner one, and the row's is on the device"
        );
        assert_eq!(
            tables.clips[deep.0 as usize].space, NO_SPACE,
            "three rounded links cannot keep a part"
        );
        assert_same_coverage(&scene, &prepared);

        // The inner content scrolls: only chains that put it on the device refresh.
        scene
            .spatial
            .place(inner, Matrix4::translation(-7.0, -44.0, 0.0));
        prepared.update(&scene);
        assert_eq!(prepared.dirty().clips.slots, vec![deep.0]);
        assert_same_coverage(&scene, &prepared);

        // The outer content scrolls: the chains below it that keep the inner content refresh,
        // because they put the outer content on the device.
        scene
            .spatial
            .place(outer, Matrix4::translation(0.0, -9.5, 0.0));
        prepared.update(&scene);
        let mut dirtied = prepared.dirty().clips.slots.clone();
        dirtied.sort_unstable();
        assert_eq!(dirtied, vec![cell.0, deep.0]);
        assert_same_coverage(&scene, &prepared);
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
    fn a_sweep_that_retracts_the_id_space_is_absorbed() {
        let mut scene = zgui_scene::Scene::new();
        let kept = scene.clips.only(ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
        let doomed = scene.clips.only(ClipLink::rect(rect(5.0, 5.0, 20.0, 20.0)));
        assert!(doomed.0 > kept.0, "the doomed chain holds the tail slot");
        let mut prepared = PreparedTables::default();
        prepared.update(&scene);

        // The kept chain stays in use while the tail chain ages out; the sweep then frees the
        // tail slot, retracts the id space over it, and journals the freeing.
        for _ in 0..12 {
            scene.clips.begin_frame();
            scene.clips.only(ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
        }
        assert!(scene.clips.evict_unreachable_chains(8, usize::MAX) >= 1);
        assert!(
            scene.clips.slots() <= doomed.0 as usize,
            "the id space retracted over the freed tail"
        );

        // The journaled slot lies past the table's end now; preparing must absorb that.
        prepared.update(&scene);
        assert_eq!(prepared.tables().clips.len(), scene.clips.slots());
        assert!(
            prepared
                .dirty()
                .clips
                .slots
                .iter()
                .all(|slot| (*slot as usize) < scene.clips.slots())
        );
        assert_eq!(
            prepared.tables().clips[kept.0 as usize],
            gpu_clip(&scene.clips.resolve(kept)),
            "the surviving chain still resolves"
        );
    }

    #[test]
    fn a_slot_reused_for_a_different_chain_follows_its_new_space() {
        let mut scene = zgui_scene::Scene::new();
        let doomed = scene.clips.only(ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
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
            OwnSpace::of(Some(Matrix4::scale(2.0, 1.0, 1.0)), None, false),
        );
        let reused = scene.clips.only(ClipLink::RoundedRect {
            shape: zgui_scene::CornerShape::ROUND,
            rect: rect(1.0, 0.0, 10.0, 10.0),
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
            OwnSpace::of(Some(Matrix4::scale(7.0, 1.0, 1.0)), None, false),
        );
        prepared.update(&scene);
        assert_eq!(prepared.dirty().clips.slots, vec![reused.0]);
        assert_eq!(prepared.tables().clips[reused.0 as usize].aabb[0], 7.0);
    }
}
