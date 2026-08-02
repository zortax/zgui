//! The persistent, content-interning side table the clip and paint id spaces are built on.

pub mod id;

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use zgui_bits::EpochBitset;

use crate::content::Content;

pub use crate::table::id::TableId;

/// One interned value and its bookkeeping.
#[derive(Clone, Debug)]
struct Entry<V> {
    /// The value itself.
    value: V,
    /// Its content hash, kept so a lookup narrows before it compares, and so a caller can ask
    /// whether an id still resolves to the content it was handed out for.
    hash: u64,
    /// How many long-lived things refer to this id.
    refs: u32,
    /// The frame generation it was last used in.
    generation: u64,
    /// Whether it may never be evicted, however cold it gets.
    pinned: bool,
}

/// A map from content to a stable id, kept across frames.
///
/// Interning the same content twice returns the same id, and an id keeps resolving to its content
/// for as long as anything refers to it. Both halves matter, and the second is the one that is easy
/// to lose: a fragment whose paint operations were recorded last frame carries *last frame's*
/// indices, and a table rebuilt per frame would resolve them to whatever happened to land in the
/// same slot.
///
/// Entries are marked as used when they are interned or read, and
/// [`Table::evict_least_recently_used`] frees exactly the coldest generation of entries that
/// nothing refers to and this frame did not touch.
///
/// ```
/// use zgui_scene::{ClipId, Table};
/// use zgui_geom::Matrix4;
///
/// let mut table: Table<ClipId, Matrix4> = Table::new();
/// let first = table.intern(Matrix4::translation(4.0, 0.0, 0.0));
/// let again = table.intern(Matrix4::translation(4.0, 0.0, 0.0));
/// assert_eq!(first, again, "the same content is the same id");
/// assert_eq!(table.len(), 1);
/// ```
#[derive(Clone, Debug)]
pub struct Table<K: TableId, V: Content> {
    /// Every entry, `None` where a slot has been freed.
    entries: Vec<Option<Entry<V>>>,
    /// Slots of `entries` that are free.
    free: Vec<u32>,
    /// Content hash to the slots holding it, so a lookup compares a handful of candidates rather
    /// than the whole table.
    by_hash: FxHashMap<u64, SmallVec<[u32; 2]>>,
    /// Which slots this frame has touched.
    used: EpochBitset,
    /// The current frame generation.
    generation: u64,
    /// Ties the table to its id type without storing one.
    key: core::marker::PhantomData<fn() -> K>,
}

impl<K: TableId, V: Content> Default for Table<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: TableId, V: Content> Table<K, V> {
    /// An empty table.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free: Vec::new(),
            by_hash: FxHashMap::default(),
            used: EpochBitset::new(),
            generation: 0,
            key: core::marker::PhantomData,
        }
    }

    /// Starts a new frame: a new generation, and nothing marked as used in it yet.
    pub fn begin_frame(&mut self) {
        self.generation += 1;
        self.used.bump();
    }

    /// The generation entries interned or read right now are stamped with.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// How many entries the table holds — the count of *live* ones, holes excluded.
    ///
    /// This is not a bound on the ids the table has handed out. An id is a slot, freeing one leaves
    /// a hole behind, and anything walking the id space wants [`Table::slots`] instead: the two
    /// differ by exactly the number of free slots the moment anything is freed.
    pub fn len(&self) -> usize {
        self.entries.iter().flatten().count()
    }

    /// How far the id space reaches: one past the highest slot the table has ever handed out.
    ///
    /// Every id is below this, live or freed, so this — never [`Table::len`] — is what bounds a walk
    /// of the id space, such as flattening the table into an array a shader indexes by id.
    pub fn slots(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table holds nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The id for `value`, interning it if this is the first time it has been seen.
    ///
    /// Marks the entry as used this frame either way, so interning a value is enough to keep it
    /// alive through the frame that draws with it.
    ///
    /// An entry already holding `value` takes it anyway. For a value whose equality is its whole
    /// content that is a write of the bytes already there; for one that is named by less than it
    /// holds — a clip rectangle named by which rectangle of the document it is and holding where
    /// that has been scrolled to — it is how the entry comes to say where the thing is now. Either
    /// way the entry keeps the identity it was interned under, which is the promise an id handed
    /// out in an earlier frame rests on.
    pub fn intern(&mut self, value: V) -> K {
        let hash = value.content_hash();
        if let Some(slot) = self.lookup(hash, &value) {
            if let Some(entry) = self.entries[slot as usize].as_mut() {
                entry.value = value;
            }
            self.touch(slot);
            return K::from_index(slot);
        }
        let entry = Entry {
            value,
            hash,
            refs: 0,
            generation: self.generation,
            pinned: false,
        };
        let slot = match self.free.pop() {
            Some(slot) => {
                self.entries[slot as usize] = Some(entry);
                slot
            }
            None => {
                self.entries.push(Some(entry));
                (self.entries.len() - 1) as u32
            }
        };
        self.by_hash.entry(hash).or_default().push(slot);
        self.touch(slot);
        K::from_index(slot)
    }

    /// The value `id` resolves to, without marking it as used.
    ///
    /// Reading without marking is what a transcript or an assertion wants; a frame that is going to
    /// *draw* with the value should call [`Table::use_of`] so eviction can see it.
    pub fn get(&self, id: K) -> Option<&V> {
        self.entries
            .get(id.index() as usize)?
            .as_ref()
            .map(|entry| &entry.value)
    }

    /// The value `id` resolves to, marking it as used this frame.
    pub fn use_of(&mut self, id: K) -> Option<&V> {
        let slot = id.index();
        if self.entries.get(slot as usize)?.is_some() {
            self.touch(slot);
        }
        self.entries[slot as usize]
            .as_ref()
            .map(|entry| &entry.value)
    }

    /// The content hash `id` currently resolves to.
    ///
    /// An id handed out in one frame must still resolve to the same hash in a later frame while
    /// anything refers to it. That is the property recorded paint operations depend on, and this is
    /// how a test states it.
    pub fn content_hash(&self, id: K) -> Option<u64> {
        self.entries
            .get(id.index() as usize)?
            .as_ref()
            .map(|entry| entry.hash)
    }

    /// Whether `id` still resolves to anything.
    pub fn contains(&self, id: K) -> bool {
        self.entries
            .get(id.index() as usize)
            .is_some_and(Option::is_some)
    }

    /// Holds `id` against eviction, and reports whether it was there to hold.
    ///
    /// The count saturates rather than wrapping, so an entry that is somehow retained more times
    /// than a `u32` can count stays retained instead of becoming evictable while in use.
    pub fn retain(&mut self, id: K) -> bool {
        match self
            .entries
            .get_mut(id.index() as usize)
            .and_then(Option::as_mut)
        {
            Some(entry) => {
                entry.refs = entry.refs.saturating_add(1);
                true
            }
            None => false,
        }
    }

    /// Releases one hold on `id`, and reports whether it was there to release.
    ///
    /// Releasing an entry nobody holds is a no-op, never an underflow.
    pub fn release(&mut self, id: K) -> bool {
        match self
            .entries
            .get_mut(id.index() as usize)
            .and_then(Option::as_mut)
        {
            Some(entry) => {
                entry.refs = entry.refs.saturating_sub(1);
                true
            }
            None => false,
        }
    }

    /// How many holds `id` has, or `None` when it resolves to nothing.
    pub fn refs(&self, id: K) -> Option<u32> {
        self.entries
            .get(id.index() as usize)?
            .as_ref()
            .map(|entry| entry.refs)
    }

    /// Makes `id` permanent: it is never evicted, whatever its generation.
    ///
    /// This is for the entries a table cannot be without — the chain that clips nothing, the
    /// transform that moves nothing — whose ids are compile-time constants and so must never be
    /// reused for anything else.
    pub fn pin(&mut self, id: K) -> bool {
        match self
            .entries
            .get_mut(id.index() as usize)
            .and_then(Option::as_mut)
        {
            Some(entry) => {
                entry.pinned = true;
                true
            }
            None => false,
        }
    }

    /// Frees exactly the coldest generation of entries that nothing holds and this frame did not
    /// touch, and reports how many went.
    ///
    /// Stepping one generation at a time rather than sweeping to a watermark means a caller that
    /// needs more room can see what each step bought.
    pub fn evict_least_recently_used(&mut self) -> usize {
        let Some(generation) = self.evictable().map(|(_, age)| age).min() else {
            return 0;
        };
        let doomed: Vec<u32> = self
            .evictable()
            .filter(|(_, age)| *age == generation)
            .map(|(slot, _)| slot)
            .collect();
        for slot in &doomed {
            self.free_slot(*slot);
        }
        doomed.len()
    }

    /// Every slot that could be evicted, with its generation.
    fn evictable(&self) -> impl Iterator<Item = (u32, u64)> + '_ {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(slot, held)| held.as_ref().map(|entry| (slot, entry)))
            .filter(|(slot, entry)| !entry.pinned && entry.refs == 0 && !self.used.contains(*slot))
            .map(|(slot, entry)| (slot as u32, entry.generation))
    }

    /// Drops one slot and everything pointing at it.
    fn free_slot(&mut self, slot: u32) {
        let Some(entry) = self.entries[slot as usize].take() else {
            return;
        };
        if let Some(bucket) = self.by_hash.get_mut(&entry.hash) {
            bucket.retain(|candidate| *candidate != slot);
            if bucket.is_empty() {
                self.by_hash.remove(&entry.hash);
            }
        }
        self.used.forget(slot as usize);
        self.free.push(slot);
    }

    /// The slot holding `value`, if any.
    fn lookup(&self, hash: u64, value: &V) -> Option<u32> {
        self.by_hash.get(&hash)?.iter().copied().find(|slot| {
            self.entries[*slot as usize]
                .as_ref()
                .is_some_and(|entry| entry.value == *value)
        })
    }

    /// Marks a slot as used this frame and moves it into the current generation.
    fn touch(&mut self, slot: u32) {
        self.used.visit(slot as usize);
        if let Some(entry) = self.entries[slot as usize].as_mut() {
            entry.generation = self.generation;
        }
    }
}
