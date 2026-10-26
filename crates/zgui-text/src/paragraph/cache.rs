//! Holding shaped paragraphs across frames, and the protocol that uses them.

use core::cell::Cell;
use std::collections::BTreeSet;

use rustc_hash::FxHashMap;
use zgui_profile::{Counter, counter};

use crate::paragraph::break_request::BreakRequest;
use crate::paragraph::broken::BrokenParagraph;
use crate::paragraph::content::ParagraphContent;
use crate::paragraph::key::ParagraphKey;
use crate::paragraph::shaped::ShapedParagraph;
use crate::paragraph::shaper::ParagraphShaper;

/// Shaped paragraphs, held under the key their content and styles hash to.
///
/// Entries live across frames, because the whole value of the split is that a paragraph shaped for
/// one frame is broken again and again in later ones without being reshaped.
#[derive(Debug, Default)]
pub struct ParagraphCache<E> {
    /// The shaped results held.
    entries: FxHashMap<ParagraphKey, Entry<E>>,
    /// Keys that can have no current resolution and that were not examined yet.
    ///
    /// A key comes here when it is inserted and when its owner reports it released. Only these
    /// keys are examined, so a frame that changes one paragraph costs a few checks. A key can be
    /// here more than once, and a key that was removed stays here until the next examination or
    /// compaction. A list empties in constant time, so a frame that changes nothing writes
    /// nothing here.
    unchecked: Vec<ParagraphKey>,
    /// Held keys that had no current resolution when they were examined, under the access stamp
    /// they had then, coldest first.
    ///
    /// A stamp can become old when the entry is read again. Eviction checks the stamp of each
    /// entry it takes from here and files the entry again under its new stamp when they differ.
    idle: BTreeSet<(u64, ParagraphKey)>,
    /// The stamp each entry in `idle` is filed under.
    ///
    /// Kept apart from `entries`, so the entries that layout reads stay small.
    filed: FxHashMap<ParagraphKey, u64>,
    /// How many lookups have found a held result since the cache was built.
    ///
    /// Monotonic and never reset, because the question it is kept for is asked between two moments
    /// rather than about one: a reader subtracts two readings to learn whether anything read the
    /// cache in between, and a per-frame figure would need the cache to be told when a frame is.
    hits: Cell<u64>,
    /// Monotonic access order for entry-local LRU eviction.
    clock: Cell<u64>,
    /// The sum of the engine byte estimates the held results carry.
    engine_bytes: usize,
}

#[derive(Debug)]
struct Entry<E> {
    shaped: ShapedParagraph<E>,
    last_used: Cell<u64>,
}

/// The capacity `unchecked` keeps after an examination. A list with more than four times this
/// capacity, such as the one the first frame fills, gives its memory back.
const UNCHECKED_KEEP: usize = 256;

impl<E> ParagraphCache<E> {
    /// An empty cache.
    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
            unchecked: Vec::new(),
            idle: BTreeSet::new(),
            filed: FxHashMap::default(),
            hits: Cell::new(0),
            clock: Cell::new(0),
            engine_bytes: 0,
        }
    }

    /// The sum of the engine byte estimates that the held results carry.
    ///
    /// Zero unless the owner records estimates with [`ParagraphCache::restate_engine_bytes`].
    pub fn engine_bytes(&self) -> usize {
        self.engine_bytes
    }

    /// Records what `estimate` answers as the engine byte estimate of the result held under `key`.
    ///
    /// Counts no lookup. The estimate moves with the result, so a result taken out and put back
    /// keeps it.
    pub fn restate_engine_bytes(
        &mut self,
        key: ParagraphKey,
        estimate: impl FnOnce(&ShapedParagraph<E>) -> usize,
    ) {
        let Some(entry) = self.entries.get_mut(&key) else {
            return;
        };
        let bytes = estimate(&entry.shaped);
        self.engine_bytes = self.engine_bytes - entry.shaped.engine_bytes() + bytes;
        entry.shaped.set_engine_bytes(bytes);
    }

    /// How many lookups have found a held result since the cache was built.
    pub fn hits(&self) -> u64 {
        self.hits.get()
    }

    /// How many shaped results are held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether a key is held.
    pub fn holds(&self, key: ParagraphKey) -> bool {
        self.entries.contains_key(&key)
    }

    /// Which break the result held under one key reflects, read without counting a lookup.
    pub fn broken_as(&self, key: ParagraphKey) -> Option<zgui_text_style::BreakingKey> {
        self.entries.get(&key)?.shaped.broken_as()
    }

    /// The result held under one key.
    pub fn get(&self, key: ParagraphKey) -> Option<&ShapedParagraph<E>> {
        let held = self.entries.get(&key);
        if let Some(held) = held {
            self.hits.set(self.hits.get() + 1);
            held.last_used.set(self.tick());
        }
        held.map(|entry| &entry.shaped)
    }

    /// The result held under one key, for a breaking pass.
    ///
    /// Breaking is what a shaped result is *for*, and it mutates it: the glyphs record which break
    /// they currently reflect, which is what makes the next request with the same key free.
    pub fn get_mut(&mut self, key: ParagraphKey) -> Option<&mut ShapedParagraph<E>> {
        let tick = self.tick();
        let held = self.entries.get_mut(&key)?;
        let hits = self.hits.get_mut();
        *hits += 1;
        held.last_used.set(tick);
        Some(&mut held.shaped)
    }

    /// Holds one shaped result, under the key it carries.
    ///
    /// The key is the result's own rather than an argument, so a caller cannot file a paragraph
    /// under a key that does not describe it and serve it to a different one later.
    pub fn insert(&mut self, shaped: ShapedParagraph<E>) {
        let last_used = self.tick();
        let key = shaped.key();
        self.engine_bytes += shaped.engine_bytes();
        let replaced = self.entries.insert(
            key,
            Entry {
                shaped,
                last_used: Cell::new(last_used),
            },
        );
        if let Some(replaced) = replaced {
            self.engine_bytes -= replaced.shaped.engine_bytes();
            self.unfile(key);
        }
        self.mark_unchecked(key);
    }

    /// Takes every shaped result out, in no particular order.
    ///
    /// For a worker cache being absorbed into the one the frame reads: the entries move whole,
    /// break state included, so the lines a worker kept are the lines later stages find.
    pub fn drain_shaped(&mut self) -> impl Iterator<Item = ShapedParagraph<E>> + '_ {
        self.engine_bytes = 0;
        self.unchecked = Vec::new();
        self.idle.clear();
        self.filed.clear();
        self.entries.drain().map(|(_, entry)| entry.shaped)
    }

    /// Takes one shaped result out, if it is held.
    ///
    /// The ownership move a batch worker needs: breaking mutates the entry, so the worker that
    /// will break a paragraph has to own it for the duration, and the frame's cache gets it back
    /// when the worker is absorbed.
    pub fn take(&mut self, key: ParagraphKey) -> Option<ShapedParagraph<E>> {
        self.remove(key)
    }

    /// Drops everything, and reports how many results that threw away.
    ///
    /// The count is the whole answer to "is anything now stale": every measurement taken from a
    /// dropped result is one, and a cache that held nothing leaves nothing behind to invalidate.
    pub fn clear(&mut self) -> usize {
        let held = self.entries.len();
        self.entries.clear();
        self.unchecked = Vec::new();
        self.idle.clear();
        self.filed.clear();
        self.engine_bytes = 0;
        held
    }

    /// Drops one entry.
    pub fn forget(&mut self, key: ParagraphKey) {
        self.remove(key);
    }

    /// Records that the last current resolution of each of `keys` went away, and files the
    /// entries `is_active` rejects as eviction candidates.
    ///
    /// `is_active` tells whether a current layout resolution names a key. `None` says that the
    /// owner lost count of which keys it released, and examines every held key once. A key that
    /// is not held contributes nothing. Called once per frame, this keeps the candidates current
    /// at a cost that follows what the frame inserted and released.
    pub fn note_released(
        &mut self,
        keys: Option<&[ParagraphKey]>,
        is_active: impl Fn(ParagraphKey) -> bool,
    ) {
        match keys {
            Some(keys) => self.unchecked.extend_from_slice(keys),
            None => self.unchecked.extend(self.entries.keys().copied()),
        }
        let examined = self.file_unchecked(&is_active);
        counter::add(Counter::ParagraphsExamined, examined);
    }

    /// Drops up to `count` least-recently-used entries for which `is_active` is false.
    ///
    /// `is_active` tells whether a current layout resolution names a key. The cache examines only
    /// the keys inserted or released since they were last examined and the candidates it drops,
    /// so the cost follows what changed. This is correct only while every key whose resolution goes
    /// away is reported through [`ParagraphCache::note_released`].
    pub fn evict_inactive(
        &mut self,
        is_active: impl Fn(ParagraphKey) -> bool,
        count: usize,
    ) -> usize {
        if count == 0 || self.entries.is_empty() {
            return 0;
        }
        let mut examined = self.file_unchecked(&is_active);
        let mut dropped = 0;
        while dropped < count {
            let Some((stamp, key)) = self.idle.pop_first() else {
                break;
            };
            examined += 1;
            self.filed.remove(&key);
            let Some(entry) = self.entries.get(&key) else {
                continue;
            };
            if is_active(key) {
                continue;
            }
            // Read since it was filed: file it again under the stamp it has now, so the order
            // stays least-recently-used.
            let now = entry.last_used.get();
            if now != stamp {
                self.filed.insert(key, now);
                self.idle.insert((now, key));
                continue;
            }
            self.remove(key);
            dropped += 1;
        }
        counter::add(Counter::ParagraphsExamined, examined);
        dropped
    }

    /// Files the unchecked entries `is_active` rejects as candidates, and reports how many it
    /// examined.
    fn file_unchecked(&mut self, is_active: &impl Fn(ParagraphKey) -> bool) -> u64 {
        if self.unchecked.is_empty() {
            return 0;
        }
        let mut keys = core::mem::take(&mut self.unchecked);
        if keys.len() > 1 {
            keys.sort_unstable();
            keys.dedup();
        }
        let mut examined = 0;
        for &key in &keys {
            let Some(entry) = self.entries.get(&key) else {
                continue;
            };
            examined += 1;
            // An entry already filed is a candidate already; an active one comes back through
            // `note_released` when its resolution goes away.
            if self.filed.contains_key(&key) || is_active(key) {
                continue;
            }
            let stamp = entry.last_used.get();
            self.filed.insert(key, stamp);
            self.idle.insert((stamp, key));
        }
        keys.clear();
        if keys.capacity() > 4 * UNCHECKED_KEEP {
            keys.shrink_to(UNCHECKED_KEEP);
        }
        self.unchecked = keys;
        examined
    }

    /// Removes one entry, and takes its estimate off the sum.
    fn remove(&mut self, key: ParagraphKey) -> Option<ShapedParagraph<E>> {
        let entry = self.entries.remove(&key)?;
        self.engine_bytes -= entry.shaped.engine_bytes();
        self.unfile(key);
        Some(entry.shaped)
    }

    /// Adds one key to `unchecked`.
    ///
    /// When the list holds at least 512 keys and twice as many keys as the cache holds, it compacts
    /// to the held keys. Thus the list stays bounded when nobody examines it.
    fn mark_unchecked(&mut self, key: ParagraphKey) {
        if self.unchecked.len() >= 2 * self.entries.len().max(UNCHECKED_KEEP) {
            self.unchecked.sort_unstable();
            self.unchecked.dedup();
            let entries = &self.entries;
            self.unchecked.retain(|key| entries.contains_key(key));
        }
        self.unchecked.push(key);
    }

    /// Takes one key out of the eviction candidates.
    fn unfile(&mut self, key: ParagraphKey) {
        if let Some(stamp) = self.filed.remove(&key) {
            self.idle.remove(&(stamp, key));
        }
    }

    fn tick(&self) -> u64 {
        let next = self.clock.get().wrapping_add(1);
        self.clock.set(next);
        next
    }
}

/// Lays out one paragraph at one width, shaping only if this content has not been shaped before.
///
/// This is the protocol the whole design exists for, in one place so that no caller has to
/// reimplement it and get the order wrong: hash the content, shape it if it is new, then break the
/// glyphs — new or not — at the width being asked about.
///
/// A width change therefore costs a break. A colour change costs neither, because it moves the
/// colour behind a brush slot and keeps the slot. A change of slot costs a shape, because the slot is
/// in the paragraph key. A `vertical-align` change costs a break, because the request carries the
/// shift.
pub fn lay_out<'cache, S: ParagraphShaper>(
    shaper: &mut S,
    cache: &'cache mut ParagraphCache<S::Engine>,
    content: &ParagraphContent<'_>,
    request: &BreakRequest<'_>,
) -> (&'cache mut ShapedParagraph<S::Engine>, BrokenParagraph) {
    let key = ParagraphKey::of(content);
    if !cache.holds(key) {
        let shaped = shaper.shape_keyed(key, content);
        cache.insert(shaped);
    }
    let shaped = cache.get_mut(key).expect("the paragraph was just cached");
    let broken = shaper.break_lines(shaped, request);
    (shaped, broken)
}

#[cfg(test)]
mod tests {
    use crate::{ContentWidths, StrutMetrics, TextMap};

    use super::{ParagraphCache, ParagraphKey, ShapedParagraph};

    fn shaped(key: u64) -> ShapedParagraph<()> {
        ShapedParagraph::new(
            ParagraphKey(key),
            key.to_string(),
            TextMap::new(),
            ContentWidths::default(),
            StrutMetrics::default(),
            [],
            (),
        )
    }

    #[test]
    fn eviction_is_lru_and_never_removes_an_active_key() {
        let mut cache = ParagraphCache::new();
        cache.insert(shaped(1));
        cache.insert(shaped(2));
        cache.insert(shaped(3));

        // One is newer than two; three is pinned independently of its age.
        assert!(cache.get(ParagraphKey(1)).is_some());
        assert_eq!(cache.evict_inactive(|key| key == ParagraphKey(3), 1), 1);
        assert!(cache.holds(ParagraphKey(1)));
        assert!(!cache.holds(ParagraphKey(2)));
        assert!(cache.holds(ParagraphKey(3)));

        assert_eq!(
            cache.evict_inactive(|key| key == ParagraphKey(3), usize::MAX),
            1
        );
        assert_eq!(cache.len(), 1);
        assert!(cache.holds(ParagraphKey(3)));
    }

    #[test]
    fn the_byte_sum_follows_every_way_in_and_out() {
        let mut cache = ParagraphCache::new();
        for key in 1..=4 {
            cache.insert(shaped(key));
            cache.restate_engine_bytes(ParagraphKey(key), |_| 100 * key as usize);
        }
        assert_eq!(cache.engine_bytes(), 1_000);

        cache.restate_engine_bytes(ParagraphKey(4), |_| 40);
        assert_eq!(cache.engine_bytes(), 640);

        // A result taken out and put back keeps its estimate.
        let taken = cache.take(ParagraphKey(3)).expect("held");
        assert_eq!(cache.engine_bytes(), 340);
        cache.insert(taken);
        assert_eq!(cache.engine_bytes(), 640);

        // A result under a held key replaces the old estimate.
        cache.insert(shaped(2));
        assert_eq!(cache.engine_bytes(), 440);

        cache.forget(ParagraphKey(1));
        assert_eq!(cache.engine_bytes(), 340);
        // Three went back in before two, so it is the colder of the two inactive results.
        assert_eq!(cache.evict_inactive(|key| key == ParagraphKey(4), 1), 1);
        assert!(!cache.holds(ParagraphKey(3)));
        assert_eq!(cache.engine_bytes(), 40);
        cache.clear();
        assert_eq!(cache.engine_bytes(), 0);
    }

    /// A key found active is examined again only after it is reported released, and then it goes.
    #[test]
    fn a_released_key_is_evicted_once_reported() {
        let mut cache = ParagraphCache::new();
        for key in 1..=3 {
            cache.insert(shaped(key));
        }
        let active = core::cell::Cell::new([true, true, true]);
        let is_active = |key: ParagraphKey| active.get()[key.0 as usize - 1];
        assert_eq!(cache.evict_inactive(is_active, usize::MAX), 0);

        // Two loses its resolution. Unreported, eviction does not see it.
        active.set([true, false, true]);
        assert_eq!(cache.evict_inactive(is_active, usize::MAX), 0);
        assert!(cache.holds(ParagraphKey(2)));

        cache.note_released(Some(&[ParagraphKey(2)]), is_active);
        assert_eq!(cache.evict_inactive(is_active, usize::MAX), 1);
        assert!(!cache.holds(ParagraphKey(2)));
        assert_eq!(cache.len(), 2);

        // A lost count makes every held key a candidate again.
        active.set([false, false, false]);
        cache.note_released(None, is_active);
        assert_eq!(cache.evict_inactive(is_active, usize::MAX), 2);
        assert!(cache.is_empty());
    }

    /// An idle entry read again goes behind the idle entries that were not.
    #[test]
    fn a_read_moves_an_idle_entry_behind_colder_ones() {
        let mut cache = ParagraphCache::new();
        for key in 1..=3 {
            cache.insert(shaped(key));
        }
        // Files one and two as idle and drops the colder.
        cache.evict_inactive(|key| key == ParagraphKey(3), 1);
        assert!(!cache.holds(ParagraphKey(1)));

        assert!(cache.get(ParagraphKey(2)).is_some());
        cache.note_released(Some(&[ParagraphKey(3)]), |_| false);
        assert_eq!(cache.evict_inactive(|_| false, 1), 1);
        assert!(cache.holds(ParagraphKey(2)), "two was read after three");
        assert!(!cache.holds(ParagraphKey(3)));
    }

    /// Each candidate is held, and `filed` names the stamp `idle` holds it under.
    fn assert_filed_exactly<E>(cache: &ParagraphCache<E>) {
        assert_eq!(cache.filed.len(), cache.idle.len());
        for (stamp, key) in &cache.idle {
            assert_eq!(cache.filed.get(key), Some(stamp));
            assert!(cache.holds(*key));
        }
    }

    /// A large first examination gives its list back, and the candidates stay exact through
    /// every way in and out.
    #[test]
    fn the_unchecked_list_shrinks_and_the_candidates_stay_exact() {
        let mut cache = ParagraphCache::new();
        for key in 1..=5_000 {
            cache.insert(shaped(key));
        }
        cache.note_released(Some(&[]), |key| key.0 % 2 == 0);
        assert!(cache.unchecked.capacity() <= super::UNCHECKED_KEEP);
        assert_eq!(cache.idle.len(), 2_500);
        assert_filed_exactly(&cache);

        // A key reported twice is filed once, and a key that is not held is not filed.
        cache.note_released(
            Some(&[ParagraphKey(2), ParagraphKey(2), ParagraphKey(9_999)]),
            |_| false,
        );
        assert_eq!(cache.idle.len(), 2_501);
        assert_filed_exactly(&cache);

        assert!(cache.get(ParagraphKey(1)).is_some());
        assert_eq!(cache.evict_inactive(|_| false, 3), 3);
        assert!(
            cache.holds(ParagraphKey(1)),
            "one was read after it was filed"
        );
        assert_filed_exactly(&cache);

        let taken = cache.take(ParagraphKey(7)).expect("held");
        assert_filed_exactly(&cache);
        cache.insert(taken);
        cache.insert(shaped(9));
        cache.forget(ParagraphKey(11));
        assert_filed_exactly(&cache);
        cache.note_released(Some(&[]), |_| false);
        assert_filed_exactly(&cache);
        assert!(cache.unchecked.is_empty());
    }

    /// A cache that nobody examines keeps its unchecked list bounded.
    #[test]
    fn an_unexamined_cache_keeps_its_list_bounded() {
        let mut cache = ParagraphCache::new();
        cache.insert(shaped(1));
        for _ in 0..10_000 {
            let taken = cache.take(ParagraphKey(1)).expect("held");
            cache.insert(taken);
        }
        assert!(cache.unchecked.len() <= 2 * super::UNCHECKED_KEEP + 1);
        assert_eq!(cache.evict_inactive(|_| false, 1), 1);
        assert!(cache.is_empty());
    }

    /// An entry taken out and put back is a candidate again, and one taken out is no longer.
    #[test]
    fn taking_and_reinserting_keeps_the_candidates_exact() {
        let mut cache = ParagraphCache::new();
        cache.insert(shaped(1));
        cache.insert(shaped(2));
        assert_eq!(cache.evict_inactive(|_| true, 1), 0);
        // Both lose their resolutions and are filed as candidates.
        cache.note_released(Some(&[ParagraphKey(1), ParagraphKey(2)]), |_| false);

        let taken = cache.take(ParagraphKey(1)).expect("held");
        assert_eq!(cache.evict_inactive(|_| false, usize::MAX), 1);
        assert!(cache.is_empty());

        cache.insert(taken);
        assert_eq!(cache.evict_inactive(|_| false, usize::MAX), 1);
        assert!(cache.is_empty());
    }
}
