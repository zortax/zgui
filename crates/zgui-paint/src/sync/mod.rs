//! Fragments recorded as compositor chunks.
//!
//! A [`CompositorSync`] attached to a [`Painter`](crate::Painter) turns the emit walk into
//! commands of a `compositor_types::Transaction`:
//!
//! - A fragment encoded this frame is recorded as the chunk of its key, replacing it.
//! - A replayed fragment that moved has its chunk's offset written.
//! - A retired fragment's chunk is removed.
//! - The tables' changes become node and slot writes, and a changed set of chunks rewrites the
//!   order tree.
//!
//! Fragments outside the frame's damage are not visited, and their chunks stay as they are.

mod key;
mod lower;
mod mirror;
mod order;

use compositor_types::{ChunkKey, OutputId, Transaction};
use rustc_hash::FxHashMap;
use zgui_geom::{Device, DevicePx, Size};
use zgui_layout::{FragKey, Fragment, LayoutStore};
use zgui_scene::{ChunkPrims, Scene};

use crate::sync::key::Part;
use crate::sync::mirror::Mirror;
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
    /// Node and slot writes from the tables' changes.
    pub table_writes: usize,
    /// Order entries replaced.
    pub order_entries: usize,
    /// Primitives of kinds the chunks do not record yet.
    pub omitted_primitives: usize,
    /// Glyph runs at a variation instance, recorded at the default instance.
    pub omitted_instances: usize,
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

    /// Writes what the walk changed in `scene`'s tables, rewrites the order tree if the set of
    /// chunks changed, and returns the transaction.
    ///
    /// Returns `None` when no transaction is open.
    pub fn close(&mut self, store: &LayoutStore, scene: &mut Scene) -> Option<Transaction> {
        let mut transaction = self.transaction.take()?;
        // The walk's replays rewrite the slots of the clips they carry along.
        self.report.table_writes += self.mirror.sync(&mut transaction, scene);
        self.report.order_entries += self.order.sync(&mut transaction, store);
        Some(transaction)
    }

    /// Returns what the sync wrote since the last `open`.
    pub fn report(&self) -> SyncReport {
        self.report
    }

    /// Forgets every chunk, node and slot, for a compositor that dropped them.
    ///
    /// The open transaction is dropped, and the next transaction mirrors everything again.
    pub fn reset(&mut self) {
        *self = Self::new(self.output);
    }

    /// Reports whether the fragment `key` is recorded.
    pub(crate) fn knows(&self, key: FragKey) -> bool {
        self.chunks.contains_key(&key)
    }

    /// Writes what the scene's tables changed since the last frame.
    pub(crate) fn mirror_tables(&mut self, scene: &mut Scene) {
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
        {
            remove(transaction, &mut self.owners, previous.key, fragment.key);
            self.report.removed += 1;
        }
        if self.owners.insert(key, fragment.key) != Some(fragment.key) {
            self.order.invalidate();
        }
        let omitted = lower::record(transaction, &mut self.mirror, scene, key, chunk);
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
                self.report.removed += 1;
                self.order.invalidate();
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
