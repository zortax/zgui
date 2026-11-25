//! Fragments recorded as compositor chunks.
//!
//! A [`CompositorSync`] attached to a [`Painter`](crate::Painter) turns the emit walk into
//! commands of an `iceblink::Transaction`:
//!
//! - A fragment encoded this frame is recorded as the chunk of its key, replacing it.
//! - A replayed fragment that moved has its chunk's offset written.
//! - A box's outline and its backdrop are chunks of their own, recorded when they changed.
//! - A retired fragment's chunks are removed at the end of the frame, unless a fragment rebuilt
//!   in its place recorded the same keys. A box that arrives where a box left records the
//!   departed box's keys.
//! - The tables' changes become node and slot writes.
//! - A box enters the order tree once a chunk in its subtree is recorded.
//! - A changed child list, set of fragments or group effect rewrites the order nodes of the boxes
//!   concerned.
//! - Nodes and slots that no chunk and no node names any more are removed.
//!
//! Fragments outside the frame's damage are not visited, and their chunks stay as they are.

mod animate;
mod check;
mod extent;
mod flush;
mod key;
mod mirror;
mod moved;
mod names;
mod order;
mod resolve;

pub use self::mirror::spaces::ScrollDrive;

pub(crate) use self::key::Part;
pub(crate) use self::mirror::images::whole;
pub(crate) use self::resolve::Resolver;

use crate::Tables;
use iceblink::{ChunkKey, OutputId, ScrollId, Transaction};
use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect, Size};
use zgui_layout::{FragKey, Fragment, LayoutStore};
use zgui_space::spatial::PropertyOwner;
use zgui_space::{ClipId, SpatialId};

use crate::emit::record::{Items, Recorder};
use crate::sync::mirror::{Held, Mirror};
use crate::sync::names::Names;
use crate::sync::order::Order;

/// The chunks, nodes and slots one output's compositor holds for the fragments of one layout.
#[derive(Debug)]
pub struct CompositorSync {
    output: OutputId,
    transaction: Option<Transaction>,
    mirror: Mirror,
    order: Order,
    names: Names,
    chunks: FxHashMap<FragKey, Chunk>,
    owners: FxHashMap<ChunkKey, FragKey>,
    extras: FxHashMap<ChunkKey, Extra>,
    outlined: FxHashMap<FragKey, ChunkKey>,
    backdropped: FxHashMap<FragKey, ChunkKey>,
    retired: FxHashSet<ChunkKey>,
    extents: extent::Extents,
    /// The device ink of the chunks the last close removed.
    removed_ink: Vec<Rect<i32, Device>>,
    moves: moved::Moves,
    report: SyncReport,
    forgotten: Option<Held>,
}

/// The chunk a fragment is recorded as.
#[derive(Clone, Copy, Debug)]
struct Chunk {
    key: ChunkKey,
    offset: [f32; 2],
}

/// The outline or the backdrop chunk of a box, and the items it was last recorded with.
#[derive(Debug)]
struct Extra {
    fragment: FragKey,
    items: Items,
}

/// What the sync wrote into the transaction since it was opened.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SyncReport {
    /// Chunks recorded.
    pub recorded: usize,
    /// Chunk offsets written.
    pub offsets: usize,
    /// Chunks removed.
    pub removed: usize,
    /// Node and slot writes from the tables' changes, and group effects written.
    pub table_writes: usize,
    /// Order entries replaced.
    pub order_entries: usize,
    /// Nodes and slots removed because no chunk and no node names them any more.
    pub released: usize,
    /// Shared parameter blocks written because they changed.
    pub params_written: usize,
    /// The largest emboldening, in device pixels, of the glyph runs of the chunks recorded,
    /// replaced, moved or removed, or `None` when none of them draws glyphs.
    ///
    /// The pixels such a change damages are the boxes of the glyphs' hinted bitmaps, which only the
    /// compositor's rasteriser knows.
    pub glyphs_changed: Option<f32>,
}

impl CompositorSync {
    /// Creates a sync that draws into `output` and has recorded nothing.
    pub fn new(output: OutputId) -> Self {
        Self {
            output,
            transaction: None,
            mirror: Mirror::default(),
            order: Order::default(),
            names: Names::default(),
            chunks: FxHashMap::default(),
            owners: FxHashMap::default(),
            extras: FxHashMap::default(),
            outlined: FxHashMap::default(),
            backdropped: FxHashMap::default(),
            retired: FxHashSet::default(),
            extents: extent::Extents::default(),
            removed_ink: Vec::new(),
            moves: moved::Moves::default(),
            report: SyncReport::default(),
            forgotten: None,
        }
    }

    /// Starts writing into `transaction`.
    ///
    /// The first transaction also creates the order tree's root and sets it as the output's root.
    pub fn open(&mut self, mut transaction: Transaction) {
        self.order.ensure_root(&mut transaction, self.output);
        self.report = SyncReport::default();
        self.transaction = Some(transaction);
    }

    /// Finishes the frame's transaction and returns it.
    ///
    /// The chunks of fragments retired this frame that no fragment recorded again are removed.
    /// Then the walk's changes to `tables` and to the order of `store` are written, and
    /// the nodes and slots nothing names any more are removed.
    ///
    /// Returns `None` when no transaction is open.
    pub fn close(&mut self, store: &LayoutStore, tables: &mut Tables) -> Option<Transaction> {
        let mut transaction = self.transaction.take()?;
        self.release_forgotten(tables);
        self.removed_ink.clear();
        for key in self.retired.drain() {
            self.owners.remove(&key);
            self.extras.remove(&key);
            transaction.remove_chunk(key);
            self.mirror.chunk_removed(key);
            if let Some((anchor, ink)) = self.extents.placed_ink(key) {
                self.removed_ink.push(on_device(ink, anchor, tables));
            }
            self.report.glyphs_changed =
                max_glyphs(self.report.glyphs_changed, self.extents.glyphs(key));
            self.extents.removed(key);
            self.report.removed += 1;
            if let Some(owner) = self.names.box_of(key.owner) {
                self.order.fragments_changed(owner);
            }
        }
        self.report.params_written += self.mirror.params.sync(&mut transaction);
        // The walk's replays rewrite the slots of the clips they carry along.
        self.report.table_writes += self.mirror.sync(&mut transaction, tables);
        let (extras, names) = (&self.extras, &self.names);
        let outlined =
            |owner: BoxKey| extras.contains_key(&key::at(names.of(owner), 0, Part::Outline));
        let backdropped =
            |owner: BoxKey| extras.contains_key(&key::at(names.of(owner), 0, Part::Backdrop));
        let order = self.order.sync(&mut order::Context {
            tx: &mut transaction,
            store,
            mirror: &mut self.mirror,
            tables,
            names,
            outlined: &outlined,
            backdropped: &backdropped,
        });
        self.report.order_entries += order.entries;
        self.report.table_writes += order.effects;
        self.report.released += self.mirror.settle(&mut transaction, tables);
        self.names.settle(|key| left(store, key));
        if zgui_layout::invariants::enabled() {
            self.check_listed(store);
        }
        Some(transaction)
    }

    /// Returns what the sync wrote since the last `open`.
    pub fn report(&self) -> SyncReport {
        self.report
    }

    /// Forgets every chunk, node and slot, for a compositor that dropped them.
    ///
    /// The open transaction is dropped, and the next transaction mirrors everything again. The
    /// zgui table holds of the forgotten mirror are given back with the next tables the sync is
    /// handed.
    pub fn reset(&mut self) {
        let mut held = self.mirror.held();
        if let Some(earlier) = self.forgotten.take() {
            held.extend(earlier);
        }
        let wake = self.mirror.params.wake().cloned();
        *self = Self::new(self.output);
        self.forgotten = Some(held);
        if let Some(wake) = wake {
            self.set_wake(wake);
        }
    }

    /// Returns the device ink of the chunks the last [`CompositorSync::close`] removed.
    pub fn removed_ink(&self) -> &[Rect<i32, Device>] {
        &self.removed_ink
    }

    /// Returns the extent the recorded chunks of fragments anchored in `space` draw over, in its
    /// device pixels.
    pub fn recorded_extent(&self, space: SpatialId) -> Option<Rect<i32, Device>> {
        self.extents.of(space.index())
    }

    /// Returns the scroll node that applies the scroll offset of the box `container`.
    ///
    /// `None` until a chunk drawn in the container's content space is recorded.
    pub fn scroll_node(&self, tables: &Tables, container: BoxKey) -> Option<ScrollId> {
        self.mirror
            .spaces
            .scroll_of(&tables.spatial, PropertyOwner::of(container))
    }

    /// Makes every window that draws a [`SharedParams`](crate::content::shader::SharedParams)
    /// recorded through this sync woken by `wake` when the block changes.
    pub fn set_wake(&mut self, wake: crate::content::shader::Wake) {
        self.mirror.params.set_wake(wake);
    }

    /// Reports whether a recorded chunk or a group effect draws a shader that reads the clock.
    pub fn reads_time(&self) -> bool {
        self.mirror.reads_time()
    }

    /// Reports whether the compositor holds an image of `key`.
    pub fn holds_image(&self, key: crate::emit::record::ImageKey) -> bool {
        self.mirror.images.holds(key)
    }

    /// Returns the box whose scroll offset the scroll node `scroll` applies.
    pub fn scroll_container(&self, scroll: ScrollId) -> Option<BoxKey> {
        let owner = self.mirror.spaces.container_of(scroll)?;
        BoxKey::from_u64(owner.get())
    }

    /// Makes the next write of the offset of `scroll` adopt the compositor's scrolling up to
    /// `serial`.
    ///
    /// The offset then goes out with `acknowledge_scroll`, which keeps the compositor's motion
    /// going, instead of `scroll_to`, which ends it. An offset that did not change is
    /// acknowledged at the value last written.
    pub fn adopt_scroll(&mut self, scroll: ScrollId, serial: u64) {
        self.mirror.spaces.adopt(scroll, serial);
    }

    /// Makes the compositor scroll every scroll node with `drive` under gestures, or only by
    /// client writes for `None`.
    ///
    /// A change is written to every scroll node by the next transaction, at its base offset.
    pub fn set_scroll_drive(&mut self, drive: Option<ScrollDrive>) {
        self.mirror.spaces.set_drive(drive);
    }

    /// Stops gestures from scrolling the box `frozen`, and lets them scroll every other box.
    pub fn set_scroll_frozen(&mut self, frozen: Option<BoxKey>) {
        self.mirror
            .spaces
            .set_frozen(frozen.map(|frozen| PropertyOwner::of(frozen).content()));
    }

    /// Gives the table holds of a mirror forgotten by `reset` back to `tables`.
    fn release_forgotten(&mut self, tables: &mut Tables) {
        if let Some(held) = self.forgotten.take() {
            held.release(tables);
        }
    }

    /// Notes the group effect the box `key` is drawn with this frame, `None` for none.
    pub(crate) fn isolate(&mut self, key: BoxKey, effect: Option<crate::emit::group::GroupEffect>) {
        self.order.isolate(key, effect);
    }

    /// Reports whether the fragment `key` is recorded.
    pub(crate) fn knows(&self, key: FragKey) -> bool {
        self.chunks.contains_key(&key)
    }

    /// Returns the resolver of the open transaction, if one is open.
    pub(crate) fn resolver(&mut self) -> Option<Resolver<'_>> {
        let transaction = self.transaction.as_mut()?;
        Some(Resolver::new(transaction, &mut self.mirror))
    }

    /// Writes what `tables` changed since the last frame.
    pub(crate) fn mirror_tables(&mut self, tables: &mut Tables) {
        self.release_forgotten(tables);
        if let Some(transaction) = &mut self.transaction {
            self.report.table_writes += self.mirror.sync(transaction, tables);
        }
    }

    /// Records `items` as the painting of `fragment`, and leaves them empty.
    ///
    /// Returns where the chunks the recording replaced or removed drew, in the coordinate system
    /// of `fragment`'s anchor.
    pub(crate) fn record(
        &mut self,
        store: &LayoutStore,
        fragment: &Fragment,
        (items, ink): (&mut Items, Rect<i32, Device>),
        tables: &mut Tables,
    ) -> Option<Rect<i32, Device>> {
        self.transaction.as_ref()?;
        let key = key::of(self.name(store, fragment.box_), store, fragment, Part::Body);
        let mut replaced = self.extents.ink(key);
        let glyphs = items
            .glyph_runs
            .iter()
            .map(|run| run.embolden)
            .reduce(f32::max);
        self.report.glyphs_changed = max_glyphs(
            max_glyphs(self.report.glyphs_changed, self.extents.glyphs(key)),
            glyphs,
        );
        let transaction = self.transaction.as_mut()?;
        if let Some(previous) = self.chunks.get(&fragment.key)
            && previous.key != key
            && remove(transaction, &mut self.owners, previous.key, fragment.key)
        {
            if let Some(old) = self.extents.ink(previous.key) {
                replaced = Some(replaced.map_or(old, |held| held.union(old)));
            }
            self.report.glyphs_changed = max_glyphs(
                self.report.glyphs_changed,
                self.extents.glyphs(previous.key),
            );
            self.mirror.chunk_removed(previous.key);
            self.extents.removed(previous.key);
            self.report.removed += 1;
        }
        // A fragment that draws nothing records no chunk, unless it is the one its chunk holds.
        if items.is_empty() && self.owners.get(&key) != Some(&fragment.key) {
            // What the walk mirrored stays named for the next chunk recorded, since an outline's
            // recording can be open around this one.
            self.chunks.insert(
                fragment.key,
                Chunk {
                    key,
                    offset: [0.0; 2],
                },
            );
            return replaced;
        }
        // A fragment rebuilt in its box's place records the key its predecessor held, and the
        // compositor compares the two recordings.
        let replaces = self.retired.remove(&key);
        if self.owners.insert(key, fragment.key) != Some(fragment.key) && !replaces {
            self.order.fragments_changed(fragment.box_);
        }
        if items.is_empty() {
            self.extents.removed(key);
        } else {
            self.extents.recorded(key, fragment.anchor, ink, glyphs);
        }
        flush::record(transaction, &mut self.mirror, tables, key, items);
        self.mirror.recorded(key);
        self.order.recorded(fragment.box_);
        self.report.recorded += 1;
        self.chunks.insert(
            fragment.key,
            Chunk {
                key,
                offset: [0.0; 2],
            },
        );
        replaced
    }

    /// Records `items` as `part` of the box whose first fragment is `fragment`: its outline or its
    /// backdrop.
    ///
    /// The chunk depends on the items:
    ///
    /// - If they are empty, the chunk is removed.
    /// - If the chunk holds them already for `fragment`, nothing is written.
    /// - Otherwise the chunk is recorded, and the items are left empty.
    pub(crate) fn record_extra(
        &mut self,
        store: &LayoutStore,
        fragment: &Fragment,
        part: Part,
        items: &mut Items,
        tables: &mut Tables,
    ) {
        if self.transaction.is_none() {
            return;
        }
        let key = key::of(self.name(store, fragment.box_), store, fragment, part);
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        let replaces = self.retired.remove(&key);
        let by_fragment = match part {
            Part::Backdrop => &mut self.backdropped,
            Part::Body | Part::Outline => &mut self.outlined,
        };
        if items.is_empty() {
            if self.extras.remove(&key).is_some() || replaces {
                self.owners.remove(&key);
                by_fragment.remove(&fragment.key);
                transaction.remove_chunk(key);
                self.mirror.chunk_removed(key);
                self.report.removed += 1;
                self.order.part_changed(fragment.box_, part);
            }
            return;
        }
        if let Some(held) = self.extras.get(&key)
            && held.fragment == fragment.key
            && held.items == *items
        {
            // The ids the emission named are the ones the chunk holds, with the chunk's own
            // space and clip.
            let mut resolver = Resolver::new(transaction, &mut self.mirror);
            resolver.space(tables, items.space.index());
            if !items.clip.is_root() {
                resolver.clip(tables, items.clip);
            }
            self.mirror.recorded(key);
            return;
        }
        if !replaces && !self.extras.contains_key(&key) {
            self.order.part_changed(fragment.box_, part);
        }
        let held = items.clone();
        flush::record(transaction, &mut self.mirror, tables, key, items);
        self.mirror.recorded(key);
        self.order.recorded(fragment.box_);
        self.report.recorded += 1;
        self.owners.insert(key, fragment.key);
        by_fragment.insert(fragment.key, key);
        self.extras.insert(
            key,
            Extra {
                fragment: fragment.key,
                items: held,
            },
        );
    }

    /// Removes the chunk of `part` of the box `owner`, its outline or its backdrop, if it has
    /// one.
    pub(crate) fn drop_extra(&mut self, owner: BoxKey, part: Part) {
        if self.extras.is_empty() {
            return;
        }
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        let key = key::at(self.names.of(owner), 0, part);
        if let Some(extra) = self.extras.remove(&key) {
            self.owners.remove(&key);
            match part {
                Part::Backdrop => self.backdropped.remove(&extra.fragment),
                Part::Body | Part::Outline => self.outlined.remove(&extra.fragment),
            };
            self.retired.remove(&key);
            transaction.remove_chunk(key);
            self.mirror.chunk_removed(key);
            self.report.removed += 1;
            self.order.part_changed(owner, part);
        }
    }

    /// Moves the chunk of `fragment` by `offset` from where it was recorded.
    ///
    /// Returns where the chunk drew before and after the move, in the coordinate system of
    /// `fragment`'s anchor, when it moved.
    pub(crate) fn replayed(
        &mut self,
        fragment: &Fragment,
        offset: Size<DevicePx, Device>,
    ) -> Option<[Rect<i32, Device>; 2]> {
        let transaction = self.transaction.as_mut()?;
        let chunk = self.chunks.get_mut(&fragment.key)?;
        if self.owners.get(&chunk.key) != Some(&fragment.key) {
            return None;
        }
        let offset = [offset.width.0, offset.height.0];
        if chunk.offset == offset {
            return None;
        }
        let before = self.extents.ink(chunk.key);
        let moved_glyphs = self.extents.glyphs(chunk.key);
        self.report.glyphs_changed = max_glyphs(self.report.glyphs_changed, moved_glyphs);
        transaction.set_offset(chunk.key, offset);
        self.extents.moved(chunk.key, offset);
        chunk.offset = offset;
        self.report.offsets += 1;
        Some([before?, self.extents.ink(chunk.key)?])
    }

    /// Marks the chunks of the fragments `keys` for removal when the frame closes, and forgets
    /// the fragments.
    pub(crate) fn retire(&mut self, keys: &[FragKey]) {
        for fragment in keys {
            self.withdraw(*fragment);
        }
    }

    /// Marks the chunks of `fragment` for removal when the frame closes.
    ///
    /// The fragment counts as unrecorded from then on.
    pub(crate) fn withdraw(&mut self, fragment: FragKey) {
        let Some(chunk) = self.chunks.remove(&fragment) else {
            self.withdraw_extras(fragment);
            return;
        };
        if self.owners.get(&chunk.key) == Some(&fragment) {
            self.retired.insert(chunk.key);
        }
        if let Some(owner) = self.names.box_of(chunk.key.owner)
            && let Some(place) = self.order.place_of(owner)
        {
            self.names.withdrawn(owner, place);
        }
        self.withdraw_extras(fragment);
    }

    /// Returns the name the chunks of the box `key` are recorded under.
    ///
    /// A box the sync has not met yet arrives first: it takes over the name of a box that left
    /// the list it joins, once its parent arrived.
    fn name(&mut self, store: &LayoutStore, key: BoxKey) -> u64 {
        if !self.met(store, key) {
            self.arrive(store, key);
        }
        self.names.of(key)
    }

    /// Reports whether the box `key` arrived, is listed in the order tree, or has a fragment
    /// recorded.
    fn met(&self, store: &LayoutStore, key: BoxKey) -> bool {
        self.names.met(key)
            || self.order.knows(key)
            || store
                .fragments_of_box(key)
                .iter()
                .any(|fragment| self.chunks.contains_key(fragment))
    }

    /// Lets the box `key` arrive after its parent, and hands it the order node of the box it
    /// takes over.
    fn arrive(&mut self, store: &LayoutStore, key: BoxKey) {
        let Some(parent) = store.get(key).and_then(|node| node.parent) else {
            self.names.arrive(key, key, |_| false);
            return;
        };
        if !self.met(store, parent) {
            self.arrive(store, parent);
        }
        if let Some(gone) = self.names.arrive(key, parent, |gone| left(store, gone)) {
            self.order.take_over(key, gone);
        }
    }

    /// Marks the outline and backdrop chunks of `fragment` for removal when the frame closes.
    fn withdraw_extras(&mut self, fragment: FragKey) {
        for extras in [&mut self.outlined, &mut self.backdropped] {
            if let Some(key) = extras.remove(&fragment)
                && self.owners.get(&key) == Some(&fragment)
            {
                self.retired.insert(key);
            }
        }
    }
}

/// Records the items `emit` records as the chunk `key` of `transaction`, with `space` and `clip`
/// as the chunk's defaults.
///
/// The nodes and slots the items refer to are created in `transaction`, and nothing remembers
/// them afterwards.
pub fn record_chunk(
    transaction: &mut Transaction,
    key: ChunkKey,
    tables: &mut Tables,
    (space, clip): (SpatialId, ClipId),
    emit: impl FnOnce(&mut Recorder<'_>),
) {
    let mut mirror = Mirror::default();
    let mut items = Items::default();
    let resolver = Resolver::new(transaction, &mut mirror);
    emit(&mut Recorder::new(
        tables,
        &mut items,
        Some(resolver),
        space,
        clip,
    ));
    flush::record(transaction, &mut mirror, tables, key, &mut items);
    mirror.recorded(key);
}

/// Reports whether the box `key` left the tree of `store`, alone or with an ancestor.
fn left(store: &LayoutStore, key: BoxKey) -> bool {
    let mut at = key;
    loop {
        let Some(node) = store.get(at) else {
            return true;
        };
        let Some(parent) = node.parent else {
            return store.root() != Some(at);
        };
        if !store
            .get(parent)
            .is_some_and(|parent| parent.children.contains(&at))
        {
            return true;
        }
        at = parent;
    }
}

/// Removes the chunk `key` if `fragment` is what it is recorded for, and reports whether it was.
fn remove(
    transaction: &mut Transaction,
    owners: &mut FxHashMap<ChunkKey, FragKey>,
    key: ChunkKey,
    fragment: FragKey,
) -> bool {
    if owners.get(&key) != Some(&fragment) {
        return false;
    }
    owners.remove(&key);
    transaction.remove_chunk(key);
    true
}

/// Returns the larger emboldening of `held` and `changed`, either of which is `None` for no glyphs.
fn max_glyphs(held: Option<f32>, changed: Option<f32>) -> Option<f32> {
    match (held, changed) {
        (Some(held), Some(changed)) => Some(held.max(changed)),
        (held, changed) => held.or(changed),
    }
}

/// Returns `ink`, in the coordinate system `anchor`, on the device as `tables` place it.
#[expect(
    clippy::cast_precision_loss,
    reason = "device coordinates are far under the 2^24 an f32 holds exactly"
)]
fn on_device(ink: Rect<i32, Device>, anchor: SpatialId, tables: &Tables) -> Rect<i32, Device> {
    let Some(matrix) = tables.spatial.resolve(anchor) else {
        return ink;
    };
    let rect = Rect::new(
        zgui_geom::Point::new(DevicePx(ink.origin.x as f32), DevicePx(ink.origin.y as f32)),
        zgui_geom::Size::new(
            DevicePx(ink.size.width as f32),
            DevicePx(ink.size.height as f32),
        ),
    );
    zgui_layout::fragment::diff::pixels(zgui_geom::transformed_bounds(&matrix, rect))
}
