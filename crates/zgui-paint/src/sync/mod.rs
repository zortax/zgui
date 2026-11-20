//! Fragments recorded as compositor chunks.
//!
//! A [`CompositorSync`] attached to a [`Painter`](crate::Painter) turns the emit walk into
//! commands of a `compositor_types::Transaction`:
//!
//! - A fragment encoded this frame is recorded as the chunk of its key, replacing it.
//! - A replayed fragment that moved has its chunk's offset written.
//! - A box's outline is a chunk of its own, recorded when it changed.
//! - A retired fragment's chunks are removed at the end of the frame, unless a fragment rebuilt
//!   in its place recorded the same keys.
//! - The tables' changes become node and slot writes, and a changed set of chunks rewrites the
//!   order tree.
//!
//! Fragments outside the frame's damage are not visited, and their chunks stay as they are.

mod flush;
mod key;
mod lower;
mod mirror;
mod order;
mod resolve;

pub(crate) use self::resolve::Resolver;

use compositor_types::{ChunkKey, OutputId, Transaction};
use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Size};
use zgui_layout::{FragKey, Fragment, LayoutStore};
use zgui_scene::{ChunkPrims, ClipId, Scene, SpatialId};

use crate::emit::record::{Items, Recorder};
use crate::sync::key::Part;
use crate::sync::mirror::{Held, Mirror};
use crate::sync::order::Order;

/// The chunks, nodes and slots one output's compositor holds for the fragments of one layout.
#[derive(Debug)]
pub struct CompositorSync {
    output: OutputId,
    transaction: Option<Transaction>,
    mirror: Mirror,
    order: Order,
    chunks: FxHashMap<FragKey, Chunk>,
    owners: FxHashMap<ChunkKey, FragKey>,
    outlines: FxHashMap<ChunkKey, Outline>,
    outlined: FxHashMap<FragKey, ChunkKey>,
    retired: FxHashSet<ChunkKey>,
    merged: Items,
    report: SyncReport,
    forgotten: Option<Held>,
}

/// The chunk a fragment is recorded as.
#[derive(Clone, Copy, Debug)]
struct Chunk {
    key: ChunkKey,
    offset: [f32; 2],
}

/// The outline chunk of a box, and the items it was last recorded with.
#[derive(Debug)]
struct Outline {
    fragment: FragKey,
    items: Items,
}

/// What the sync wrote into the transaction since it was opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
    /// Primitives of kinds the chunks do not record yet, and filter steps groups leave out.
    pub omitted_primitives: usize,
    /// Glyph runs at a variation instance, recorded at the default instance.
    pub omitted_instances: usize,
    /// Nodes and slots removed because no chunk and no node names them any more.
    pub released: usize,
}

impl CompositorSync {
    /// Creates a sync that draws into `output` and has recorded nothing.
    pub fn new(output: OutputId) -> Self {
        Self {
            output,
            transaction: None,
            mirror: Mirror::default(),
            order: Order::default(),
            chunks: FxHashMap::default(),
            owners: FxHashMap::default(),
            outlines: FxHashMap::default(),
            outlined: FxHashMap::default(),
            retired: FxHashSet::default(),
            merged: Items::default(),
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
    /// Then the walk's changes to `scene`'s tables are written, and the order tree is rewritten
    /// if the set of chunks changed.
    ///
    /// Returns `None` when no transaction is open.
    pub fn close(&mut self, store: &LayoutStore, scene: &mut Scene) -> Option<Transaction> {
        let mut transaction = self.transaction.take()?;
        for key in self.retired.drain() {
            self.owners.remove(&key);
            self.outlines.remove(&key);
            transaction.remove_chunk(key);
            self.report.removed += 1;
            self.order.invalidate();
        }
        // The walk's replays rewrite the slots of the clips they carry along.
        self.report.table_writes += self.mirror.sync(&mut transaction, scene);
        let outlines = &self.outlines;
        let outlined =
            |owner: BoxKey| outlines.contains_key(&key::at(owner.as_u64(), 0, Part::Outline));
        self.report.order_entries += self.order.sync(&mut transaction, store, &outlined);
        Some(transaction)
    }

    /// Returns what the sync wrote since the last `open`.
    pub fn report(&self) -> SyncReport {
        self.report
    }

    /// Forgets every chunk, node and slot, for a compositor that dropped them.
    ///
    /// The open transaction is dropped, and the next transaction mirrors everything again. The
    /// zgui table holds of the forgotten mirror are given back with the next scene the sync is
    /// handed.
    pub fn reset(&mut self) {
        let mut held = self.mirror.held();
        if let Some(earlier) = self.forgotten.take() {
            held.extend(earlier);
        }
        *self = Self::new(self.output);
        self.forgotten = Some(held);
    }

    /// Returns the scroll node that applies the scroll offset of the box `container`.
    ///
    /// `None` until a chunk drawn in the container's content space is recorded.
    pub fn scroll_node(&self, scene: &Scene, container: BoxKey) -> Option<ScrollId> {
        self.mirror
            .spaces
            .scroll_of(&scene.spatial, PropertyOwner::of(container))
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

    /// Gives the table holds of a mirror forgotten by `reset` back to `scene`.
    fn release_forgotten(&mut self, scene: &mut Scene) {
        if let Some(held) = self.forgotten.take() {
            held.release(scene);
        }
    }

    /// Notes the group effect the box `key` is drawn with this frame, `None` for none.
    pub(crate) fn isolate(&mut self, key: BoxKey, effect: Option<crate::emit::group::GroupEffect>) {
        if let Some(effect) = &effect {
            self.report.omitted_primitives += effect.omitted;
        }
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

    /// Writes what the scene's tables changed since the last frame.
    pub(crate) fn mirror_tables(&mut self, scene: &mut Scene) {
        self.release_forgotten(scene);
        if let Some(transaction) = &mut self.transaction {
            self.report.table_writes += self.mirror.sync(transaction, scene);
        }
    }

    /// Records `items` and the quads of `prims` as the painting of `fragment`.
    ///
    /// The other primitives of `prims` are of kinds the chunks do not record yet.
    pub(crate) fn record(
        &mut self,
        store: &LayoutStore,
        fragment: &Fragment,
        items: &Items,
        prims: &ChunkPrims,
        scene: &mut Scene,
    ) {
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        let key = key::of(store, fragment, Part::Body);
        if let Some(previous) = self.chunks.get(&fragment.key)
            && previous.key != key
            && remove(transaction, &mut self.owners, previous.key, fragment.key)
        {
            self.report.removed += 1;
            self.order.invalidate();
        }
        // A fragment rebuilt in its box's place records the key its predecessor held, and the
        // compositor compares the two recordings.
        let replaces = self.retired.remove(&key);
        if self.owners.insert(key, fragment.key) != Some(fragment.key) && !replaces {
            self.order.invalidate();
        }
        let items = if lower::holds_quads(prims) {
            let resolver = Resolver::new(transaction, &mut self.mirror);
            lower::merge(items, prims, scene, resolver, &mut self.merged);
            &self.merged
        } else {
            items
        };
        flush::record(transaction, &mut self.mirror, scene, key, items);
        self.report.recorded += 1;
        self.report.omitted_primitives += prims.len() - prims.quads.len();
        self.report.omitted_instances += items.instanced;
        self.chunks.insert(
            fragment.key,
            Chunk {
                key,
                offset: [0.0; 2],
            },
        );
    }

    /// Records `items` as the outline of the box whose first fragment is `fragment`.
    ///
    /// The outline's chunk depends on the items:
    ///
    /// - If they are empty, the chunk is removed.
    /// - If the chunk holds them already for `fragment`, nothing is written.
    /// - Otherwise the chunk is recorded.
    pub(crate) fn record_outline(
        &mut self,
        store: &LayoutStore,
        fragment: &Fragment,
        items: &Items,
        scene: &mut Scene,
    ) {
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        let key = key::of(store, fragment, Part::Outline);
        let replaces = self.retired.remove(&key);
        if items.is_empty() {
            if self.outlines.remove(&key).is_some() || replaces {
                self.owners.remove(&key);
                self.outlined.remove(&fragment.key);
                transaction.remove_chunk(key);
                self.report.removed += 1;
                self.order.invalidate();
            }
            return;
        }
        if let Some(held) = self.outlines.get(&key)
            && held.fragment == fragment.key
            && held.items == *items
        {
            return;
        }
        if !replaces && !self.outlines.contains_key(&key) {
            self.order.invalidate();
        }
        flush::record(transaction, &mut self.mirror, scene, key, items);
        self.report.recorded += 1;
        self.owners.insert(key, fragment.key);
        self.outlined.insert(fragment.key, key);
        self.outlines.insert(
            key,
            Outline {
                fragment: fragment.key,
                items: items.clone(),
            },
        );
    }

    /// Removes the outline chunk of the box `owner`, if it has one.
    pub(crate) fn drop_outline(&mut self, owner: BoxKey) {
        if self.outlines.is_empty() {
            return;
        }
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        let key = key::at(owner.as_u64(), 0, Part::Outline);
        if let Some(outline) = self.outlines.remove(&key) {
            self.owners.remove(&key);
            self.outlined.remove(&outline.fragment);
            self.retired.remove(&key);
            transaction.remove_chunk(key);
            self.report.removed += 1;
            self.order.invalidate();
        }
    }

    /// Moves the chunk of `fragment` by `offset` from where it was recorded.
    pub(crate) fn replayed(&mut self, fragment: &Fragment, offset: Size<DevicePx, Device>) {
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        let Some(chunk) = self.chunks.get_mut(&fragment.key) else {
            return;
        };
        let offset = [offset.width.0, offset.height.0];
        if chunk.offset != offset {
            transaction.set_offset(chunk.key, offset);
            chunk.offset = offset;
            self.report.offsets += 1;
        }
    }

    /// Marks the chunks of the fragments `keys` for removal when the frame closes.
    pub(crate) fn retire(&mut self, keys: &[FragKey]) {
        for fragment in keys {
            if let Some(chunk) = self.chunks.remove(fragment)
                && self.owners.get(&chunk.key) == Some(fragment)
            {
                self.retired.insert(chunk.key);
            }
            if let Some(key) = self.outlined.remove(fragment)
                && self.owners.get(&key) == Some(fragment)
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
    scene: &mut Scene,
    (space, clip): (SpatialId, ClipId),
    emit: impl FnOnce(&mut Recorder<'_>),
) {
    let mut mirror = Mirror::default();
    let mut items = Items::default();
    let resolver = Resolver::new(transaction, &mut mirror);
    emit(&mut Recorder::new(
        scene,
        &mut items,
        Some(resolver),
        space,
        clip,
    ));
    flush::record(transaction, &mut mirror, scene, key, &items);
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
