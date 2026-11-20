//! Fragments recorded as compositor chunks.
//!
//! A [`CompositorSync`] attached to a [`Painter`](crate::Painter) turns the emit walk into
//! commands of a `compositor_types::Transaction`:
//!
//! - A fragment encoded this frame is recorded as the chunk of its key, replacing it.
//! - A replayed fragment that moved has its chunk's offset written.
//! - A retired fragment's chunk is removed.
//! - The tables' changes become node and slot writes.
//! - A changed child list, set of fragments or group effect rewrites the order nodes of the boxes
//!   concerned.
//! - Nodes and slots that no chunk and no node names any more are removed.
//!
//! Fragments outside the frame's damage are not visited, and their chunks stay as they are.

mod key;
mod lower;
mod mirror;
mod order;

use compositor_types::{ChunkKey, OutputId, ScrollId, Transaction};
use rustc_hash::FxHashMap;
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Size};
use zgui_layout::{FragKey, Fragment, LayoutStore};
use zgui_scene::spatial::PropertyOwner;
use zgui_scene::{ChunkPrims, Scene};

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
    report: SyncReport,
    forgotten: Option<Held>,
}

/// The chunk a fragment is recorded as.
#[derive(Clone, Copy, Debug)]
struct Chunk {
    key: ChunkKey,
    offset: [f32; 2],
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

    /// Writes what the walk changed in `scene`'s tables and in the order of `store`, removes the
    /// nodes and slots nothing names any more, and returns the transaction.
    ///
    /// Returns `None` when no transaction is open.
    pub fn close(&mut self, store: &LayoutStore, scene: &mut Scene) -> Option<Transaction> {
        let mut transaction = self.transaction.take()?;
        self.release_forgotten(scene);
        // The walk's replays rewrite the slots of the clips they carry along.
        self.report.table_writes += self.mirror.sync(&mut transaction, scene);
        let order = self.order.sync(&mut order::Context {
            tx: &mut transaction,
            store,
            mirror: &mut self.mirror,
            scene,
        });
        self.report.order_entries += order.entries;
        self.report.table_writes += order.effects;
        self.report.released += self.mirror.settle(&mut transaction, scene);
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

    /// Writes what the scene's tables changed since the last frame.
    pub(crate) fn mirror_tables(&mut self, scene: &mut Scene) {
        self.release_forgotten(scene);
        if let Some(transaction) = &mut self.transaction {
            self.report.table_writes += self.mirror.sync(transaction, scene);
        }
    }

    /// Records the painting `chunk` of `fragment`.
    pub(crate) fn record(
        &mut self,
        store: &LayoutStore,
        fragment: &Fragment,
        chunk: &ChunkPrims,
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
            self.mirror.chunk_removed(previous.key);
            self.report.removed += 1;
        }
        if self.owners.insert(key, fragment.key) != Some(fragment.key) {
            self.order.fragments_changed(fragment.box_);
        }
        let omitted = lower::record(transaction, &mut self.mirror, scene, key, chunk);
        self.mirror.recorded(key);
        self.report.recorded += 1;
        self.report.omitted_primitives += omitted.primitives;
        self.report.omitted_instances += omitted.instances;
        self.chunks.insert(
            fragment.key,
            Chunk {
                key,
                offset: [0.0; 2],
            },
        );
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

    /// Removes the chunks of the fragments `keys`.
    pub(crate) fn retire(&mut self, keys: &[FragKey]) {
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        for fragment in keys {
            if let Some(chunk) = self.chunks.remove(fragment)
                && remove(transaction, &mut self.owners, chunk.key, *fragment)
            {
                self.mirror.chunk_removed(chunk.key);
                self.report.removed += 1;
                if let Some(owner) = BoxKey::from_u64(chunk.key.owner) {
                    self.order.fragments_changed(owner);
                }
            }
        }
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
