//! The policy: what is cached, what holds it alive, and what goes when room runs out.

pub mod evict;
pub mod limits;
pub mod report;

mod entry;
mod pool;
mod upload;

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;
use zgui_bits::EpochBitset;
use zgui_geom::{Device, Size};

use crate::atlas::entry::Entry;
use crate::atlas::pool::Pool;
use crate::atlas::upload::PendingUpload;
use crate::error::AtlasError;
use crate::key::AtlasKey;
use crate::sink::TextureSink;
use crate::sink::queue::TextureQueue;
use crate::texture::TextureKind;
use crate::tile::AtlasTile;

pub use crate::atlas::evict::Eviction;
pub use crate::atlas::limits::AtlasLimits;
pub use crate::atlas::report::AtlasReport;

/// A cache of rasterised content, packed into a small number of textures.
///
/// The atlas answers one question — "where did this content go, and if it has not been rasterised
/// yet, where should it go?" — and does the bookkeeping that keeps the answer affordable: shelf
/// packing inside each texture, reference counts so content in use is never evicted, a per-frame
/// use mark so eviction can tell cold content from hot, and a queue so uploads leave in one batch.
///
/// # Frames
///
/// Call [`Atlas::begin_frame`] once per frame. It starts a new *generation*: every entry looked up
/// during the frame is stamped with it, and eviction works from the oldest generation up. An atlas
/// that is never told about frames still works — everything simply stays in generation zero and
/// eviction becomes "free everything unreferenced".
///
/// ```
/// use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, MemorySink, TextureKind};
/// use zgui_geom::Size;
///
/// let mut sink = MemorySink::new();
/// let mut atlas = Atlas::new(AtlasLimits::default());
/// let old = AtlasKey::new(1, TextureKind::Mono);
/// let new = AtlasKey::new(2, TextureKind::Mono);
///
/// atlas.begin_frame();
/// atlas.get_or_insert(old, Size::new(4, 4), || vec![0; 16]).unwrap();
///
/// atlas.begin_frame();
/// atlas.get_or_insert(new, Size::new(4, 4), || vec![0; 16]).unwrap();
///
/// // Only the entry left behind in the older generation goes.
/// let freed = atlas.evict_least_recently_used();
/// assert_eq!(freed.tiles, 1);
/// assert!(!atlas.contains(old));
/// assert!(atlas.contains(new));
/// ```
#[derive(Debug)]
pub struct Atlas {
    /// The bounds allocation happens within.
    limits: AtlasLimits,
    /// One pool per [`TextureKind`], indexed by [`TextureKind::index`].
    pools: [Pool; TextureKind::COUNT],
    /// Every entry, `None` where a slot has been freed.
    ///
    /// Dense slots rather than a bare map because the per-frame use marking is a bitset over them:
    /// marking is one bit write with no allocation and no clearing pass between frames.
    entries: Vec<Option<Entry>>,
    /// Slots of `entries` that are free.
    free_slots: Vec<u32>,
    /// Key to slot.
    index: FxHashMap<AtlasKey, u32>,
    /// Which slots this frame has touched.
    used: EpochBitset,
    /// The current frame generation.
    generation: u64,
    /// How many lookups have found a cached raster since the atlas was built.
    ///
    /// Monotonic and never reset, so a reader that wants "was anything read between these two
    /// moments" subtracts two readings rather than asking the atlas to keep a per-frame flag it
    /// would have to be told when to clear.
    hits: u64,
    /// How many entries at least one caller is holding against eviction.
    ///
    /// Maintained as the holds are taken and released rather than counted when it is asked for. It
    /// is asked for once or twice per frame by whatever budgets this atlas, and counting it then
    /// would walk every entry — a document-sized cost, on every frame, for a figure three
    /// arithmetic operations can keep exact.
    held_tiles: usize,
    /// How many bytes those held entries weigh, by each one's own format.
    held_bytes: u64,
    /// Bytes waiting to be written.
    pending: Vec<PendingUpload>,
    /// How many bytes those pending writes hold.
    pending_bytes: u64,
    /// Texture creations and destructions waiting for something that has a device.
    device: TextureQueue,
}

impl Atlas {
    /// An empty atlas allocating within `limits`.
    pub fn new(limits: AtlasLimits) -> Self {
        Self {
            limits,
            pools: TextureKind::ALL.map(Pool::new),
            entries: Vec::new(),
            free_slots: Vec::new(),
            index: FxHashMap::default(),
            used: EpochBitset::new(),
            generation: 0,
            hits: 0,
            held_tiles: 0,
            held_bytes: 0,
            pending: Vec::new(),
            pending_bytes: 0,
            device: TextureQueue::new(),
        }
    }

    /// How many texture creations and destructions are waiting for a device.
    ///
    /// Nothing about a tile's placement depends on them having happened: allocation is arithmetic
    /// over rectangles this crate owns, and this is the record of what a device will be asked to do
    /// about it when [`Atlas::flush_uploads`] is next called.
    pub fn pending_textures(&self) -> usize {
        self.device.len()
    }

    /// The bounds allocation happens within.
    pub fn limits(&self) -> AtlasLimits {
        self.limits
    }

    /// Changes the level cold content is freed back down to.
    ///
    /// The budget an atlas is held to is a property of the window it serves rather than of the
    /// atlas — how much of the device's memory this window may have is not a question an allocator
    /// can answer — so it is settable rather than fixed at construction. Nothing is freed here:
    /// the new level takes effect the next time
    /// [`Atlas::evict_to_soft_limit`] is called.
    pub fn set_soft_bytes(&mut self, bytes: Option<u64>) {
        self.limits.soft_bytes = bytes;
    }

    /// The generation entries looked up right now are stamped with.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// How many lookups have found a cached raster since the atlas was built.
    ///
    /// A running total rather than a per-frame figure: two readings subtracted answer "did anything
    /// read this atlas between these two moments", which is what a budget deciding whether the
    /// atlas is cold needs. It counts lookups, so it says nothing about content drawn by a replayed
    /// range — a replay draws from tiles without asking for them, and what says those tiles are
    /// still on the screen is [`AtlasReport::referenced_tiles`], not this.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Starts a new frame: a new generation, and no entry marked as used in it yet.
    pub fn begin_frame(&mut self) {
        self.generation += 1;
        self.used.bump();
    }

    /// How many rasters are cached.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Whether `key` is cached, without marking it as used.
    ///
    /// Use [`Atlas::get`] when the answer is going to be drawn with; this is for assertions and
    /// diagnostics, where marking would change the thing being observed.
    pub fn contains(&self, key: AtlasKey) -> bool {
        self.index.contains_key(&key)
    }

    /// Every cached tile, in no particular order.
    ///
    /// Two tiles of one texture are always disjoint, which is the invariant a consumer that draws
    /// from several of them at once depends on.
    pub fn tiles(&self) -> impl Iterator<Item = AtlasTile> + '_ {
        self.entries.iter().flatten().map(|entry| entry.tile)
    }

    /// Where `key`'s content is, marking it as used this frame.
    pub fn get(&mut self, key: AtlasKey) -> Option<AtlasTile> {
        let slot = *self.index.get(&key)?;
        self.touch(slot);
        self.entries[slot as usize].map(|entry| entry.tile)
    }

    /// Where `key`'s content is, rasterising and uploading it first if it is not cached yet.
    ///
    /// `build` is called only on a miss, and must return exactly `size`'s worth of tightly packed
    /// texels in the pool's format — top row first, no padding between rows. The bytes are queued
    /// rather than written; [`Atlas::flush_uploads`] is what sends them.
    ///
    /// # Errors
    ///
    /// [`AtlasError::TooLarge`] when no texture this atlas may create could hold `size`;
    /// [`AtlasError::OutOfSpace`] when every texture of the pool is full, which is a signal to
    /// evict and retry; and [`AtlasError::WrongByteCount`] when `build` disagrees with `size`.
    ///
    /// Nothing here can fail on a device. Growing the pool records a texture creation rather than
    /// issuing one, so this is arithmetic over rectangles and a hash lookup — which is what lets a
    /// walk that rasterises on demand run with no device in reach. A refusal from the device
    /// arrives at [`Atlas::flush_uploads`] instead.
    pub fn get_or_insert(
        &mut self,
        key: AtlasKey,
        size: Size<i32, Device>,
        build: impl FnOnce() -> Vec<u8>,
    ) -> Result<AtlasTile, AtlasError> {
        if let Some(tile) = self.get(key) {
            return Ok(tile);
        }

        let kind = key.kind();
        let (texture, tile_id, bounds) =
            self.pools[kind.index()].allocate(size, self.limits, &mut self.device)?;

        let bytes = build();
        let expected = kind
            .format()
            .bytes_for(size.width.max(0) as u32, size.height.max(0) as u32);
        if bytes.len() as u64 != expected {
            self.pools[kind.index()].deallocate(texture, tile_id, &mut self.device);
            return Err(AtlasError::WrongByteCount {
                size,
                expected,
                actual: bytes.len() as u64,
            });
        }

        self.pending_bytes += bytes.len() as u64;
        self.pending.push(PendingUpload {
            texture,
            tile: tile_id,
            bounds,
            bytes,
        });

        let tile = AtlasTile {
            texture,
            tile: tile_id,
            bounds,
        };
        let slot = self.insert_entry(Entry {
            key,
            tile,
            size,
            refs: 0,
            generation: self.generation,
        });
        self.touch(slot);
        Ok(tile)
    }

    /// Holds `key` against eviction, and reports whether it was there to hold.
    ///
    /// The count saturates: an entry that has somehow been retained `u32::MAX` times stays
    /// retained rather than wrapping to zero and becoming evictable while still in use.
    pub fn retain(&mut self, key: AtlasKey) -> bool {
        let Some(&slot) = self.index.get(&key) else {
            return false;
        };
        if let Some(entry) = self.entries[slot as usize].as_mut() {
            let was_held = !entry.is_unreferenced();
            entry.refs = entry.refs.saturating_add(1);
            if !was_held {
                let bytes = entry_bytes(entry);
                self.held_tiles += 1;
                self.held_bytes += bytes;
            }
        }
        true
    }

    /// Releases one hold on `key`, and reports whether it was there to release.
    ///
    /// Releasing an entry nobody holds is a no-op rather than an underflow: the count never wraps
    /// to `u32::MAX` and so never makes a cold entry permanently un-evictable.
    pub fn release(&mut self, key: AtlasKey) -> bool {
        let Some(&slot) = self.index.get(&key) else {
            return false;
        };
        if let Some(entry) = self.entries[slot as usize].as_mut() {
            entry.refs = entry.refs.saturating_sub(1);
            if entry.is_unreferenced() {
                let bytes = entry_bytes(entry);
                self.held_tiles = self.held_tiles.saturating_sub(1);
                self.held_bytes = self.held_bytes.saturating_sub(bytes);
            }
        }
        true
    }

    /// How many holds `key` has, or `None` when it is not cached.
    pub fn refs(&self, key: AtlasKey) -> Option<u32> {
        let slot = *self.index.get(&key)?;
        self.entries[slot as usize].map(|entry| entry.refs)
    }

    /// Drops `key`, returning its space to the allocator it came from.
    ///
    /// Returns whether anything was there to drop. Any queued upload for the tile is discarded
    /// with it: the rectangle may be handed straight back out, and a stale write would land in
    /// whatever content took its place.
    pub fn remove(&mut self, key: AtlasKey) -> bool {
        let Some(slot) = self.index.remove(&key) else {
            return false;
        };
        let Some(entry) = self.entries[slot as usize].take() else {
            return false;
        };
        if !entry.is_unreferenced() {
            // Removing an entry somebody holds is what a lost device does; the hold goes with the
            // entry, and a total that did not follow it would count a tile that no longer exists.
            self.held_tiles = self.held_tiles.saturating_sub(1);
            self.held_bytes = self.held_bytes.saturating_sub(entry_bytes(&entry));
        }
        self.free_slots.push(slot);
        self.used.forget(slot as usize);
        self.discard_pending(entry.tile);
        self.pools[entry.tile.texture.kind.index()].deallocate(
            entry.tile.texture,
            entry.tile.tile,
            &mut self.device,
        );
        true
    }

    /// Performs every texture creation, write and destruction the atlas has decided on.
    ///
    /// Returns how many bytes were written. Textures are dealt with first and in the order they
    /// were decided on, because a write into one that does not exist yet is exactly what the sink's
    /// contract promises never happens. A failure leaves the remainder queued, so a caller that
    /// recovers flushes again rather than having lost them.
    ///
    /// # Errors
    ///
    /// [`AtlasError::Sink`] when the sink refuses a creation or a write.
    pub fn flush_uploads(&mut self, sink: &mut impl TextureSink) -> Result<u64, AtlasError> {
        // Before a single byte: a write into a texture that has not been created yet is the one
        // ordering this queue can get wrong, and the sink's contract says it never happens.
        self.device.replay(sink)?;
        sink.begin_uploads()?;
        let mut written = 0;
        let mut flushed = 0;
        let mut failure = None;
        for index in 0..self.pending.len() {
            let upload = &self.pending[index];
            match sink.write_texture(
                upload.texture,
                upload.bounds,
                upload.texture.format(),
                &upload.bytes,
            ) {
                Ok(()) => {
                    written += upload.bytes.len() as u64;
                    flushed += 1;
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        // Accepted writes have to leave even when a later one failed; only those writes are
        // drained below, and the rest remain queued for the next attempt.
        sink.finish_uploads();
        self.pending.drain(..flushed);
        self.pending_bytes -= written;
        match failure {
            Some(error) => Err(error.into()),
            None => Ok(written),
        }
    }

    /// Drops everything and destroys every texture.
    ///
    /// This is the device-loss path: the allocator and the keys are ours and would survive, but the
    /// texels do not, so keeping the entries would mean handing out tiles whose contents no longer
    /// exist. Content is re-rasterised on demand afterwards. Anything holding a tile across the
    /// call is holding a rectangle of a texture that no longer exists, and has to be told: this
    /// crate names content and never says how long a name is good for.
    pub fn clear(&mut self) {
        for pool in &mut self.pools {
            pool.clear(&mut self.device);
        }
        self.entries.clear();
        self.free_slots.clear();
        self.index.clear();
        self.used.reset();
        self.held_tiles = 0;
        self.held_bytes = 0;
        self.pending.clear();
        self.pending_bytes = 0;
    }

    /// How many bytes of texture memory the atlas is holding.
    ///
    /// The textures rather than the tiles inside them: a texture costs what it costs whether it is
    /// full or holds one glyph, so this is the number a memory budget is written against and the
    /// one [`AtlasLimits::soft_bytes`] is compared to. It falls only when a texture empties
    /// completely, because that is when the memory is actually given back.
    pub fn resident_bytes(&self) -> u64 {
        TextureKind::ALL
            .iter()
            .map(|kind| {
                self.pools[kind.index()].texels() * u64::from(kind.format().bytes_per_texel())
            })
            .sum()
    }

    /// What the atlas is currently holding.
    pub fn report(&self) -> AtlasReport {
        let mut textures = 0;
        let mut texels = 0;
        let mut bytes = 0;
        for kind in TextureKind::ALL {
            let pool = &self.pools[kind.index()];
            textures += pool.live_textures();
            texels += pool.texels();
            bytes += pool.texels() * u64::from(kind.format().bytes_per_texel());
        }
        AtlasReport {
            tiles: self.index.len(),
            referenced_tiles: self.held_tiles,
            referenced_bytes: self.held_bytes,
            textures,
            texels,
            bytes,
            pending_uploads: self.pending.len(),
            pending_bytes: self.pending_bytes,
        }
    }

    /// Whether every texture has had all of its allocated space returned.
    ///
    /// True of a fresh atlas and of one that has served and dropped any number of tiles. False the
    /// moment a single tile's space is not returned, which is what makes it the assertion a leak
    /// test is written as.
    pub fn is_fully_reclaimed(&self) -> bool {
        self.pools.iter().all(Pool::is_fully_reclaimed)
    }

    /// Marks a slot as used this frame and moves its entry to the current generation.
    fn touch(&mut self, slot: u32) {
        self.hits += 1;
        self.used.visit(slot as usize);
        if let Some(entry) = self.entries[slot as usize].as_mut() {
            entry.generation = self.generation;
        }
    }

    /// Files an entry into a free slot, or a new one.
    fn insert_entry(&mut self, entry: Entry) -> u32 {
        let slot = match self.free_slots.pop() {
            Some(slot) => {
                self.entries[slot as usize] = Some(entry);
                slot
            }
            None => {
                self.entries.push(Some(entry));
                (self.entries.len() - 1) as u32
            }
        };
        self.index.insert(entry.key, slot);
        slot
    }

    /// Drops any queued upload targeting `tile`.
    fn discard_pending(&mut self, tile: AtlasTile) {
        let mut discarded = 0;
        self.pending.retain(|upload| {
            let stale = upload.texture == tile.texture && upload.tile == tile.tile;
            if stale {
                discarded += upload.bytes.len() as u64;
            }
            !stale
        });
        self.pending_bytes -= discarded;
    }
}

/// How many bytes one entry's texels weigh, in the format of the pool it sits in.
fn entry_bytes(entry: &Entry) -> u64 {
    entry.tile.texture.format().bytes_for(
        entry.size.width.max(0) as u32,
        entry.size.height.max(0) as u32,
    )
}
