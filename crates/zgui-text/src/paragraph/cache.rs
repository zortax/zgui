//! Holding shaped paragraphs across frames, and the protocol that uses them.

use core::cell::Cell;

use rustc_hash::{FxHashMap, FxHashSet};

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
    /// How many lookups have found a held result since the cache was built.
    ///
    /// Monotonic and never reset, because the question it is kept for is asked between two moments
    /// rather than about one: a reader subtracts two readings to learn whether anything read the
    /// cache in between, and a per-frame figure would need the cache to be told when a frame is.
    hits: Cell<u64>,
    /// Monotonic access order for entry-local LRU eviction.
    clock: Cell<u64>,
}

#[derive(Debug)]
struct Entry<E> {
    shaped: ShapedParagraph<E>,
    last_used: Cell<u64>,
}

impl<E> ParagraphCache<E> {
    /// An empty cache.
    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
            hits: Cell::new(0),
            clock: Cell::new(0),
        }
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
        self.entries.insert(
            shaped.key(),
            Entry {
                shaped,
                last_used: Cell::new(last_used),
            },
        );
    }

    /// Takes every shaped result out, in no particular order.
    ///
    /// For a worker cache being absorbed into the one the frame reads: the entries move whole,
    /// break state included, so the lines a worker kept are the lines later stages find.
    pub fn drain_shaped(&mut self) -> impl Iterator<Item = ShapedParagraph<E>> + '_ {
        self.entries.drain().map(|(_, entry)| entry.shaped)
    }

    /// Takes one shaped result out, if it is held.
    ///
    /// The ownership move a batch worker needs: breaking mutates the entry, so the worker that
    /// will break a paragraph has to own it for the duration, and the frame's cache gets it back
    /// when the worker is absorbed.
    pub fn take(&mut self, key: ParagraphKey) -> Option<ShapedParagraph<E>> {
        self.entries.remove(&key).map(|entry| entry.shaped)
    }

    /// Drops everything, and reports how many results that threw away.
    ///
    /// The count is the whole answer to "is anything now stale": every measurement taken from a
    /// dropped result is one, and a cache that held nothing leaves nothing behind to invalidate.
    pub fn clear(&mut self) -> usize {
        let held = self.entries.len();
        self.entries.clear();
        held
    }

    /// Drops one entry.
    pub fn forget(&mut self, key: ParagraphKey) {
        self.entries.remove(&key);
    }

    /// Drops up to `count` least-recently-used entries not named by an active layout resolution.
    pub fn evict_inactive(&mut self, active: &[ParagraphKey], count: usize) -> usize {
        if count == 0 || self.entries.is_empty() {
            return 0;
        }
        let active: FxHashSet<ParagraphKey> = active.iter().copied().collect();
        let mut candidates: Vec<(u64, ParagraphKey)> = self
            .entries
            .iter()
            .filter(|(key, _)| !active.contains(key))
            .map(|(&key, entry)| (entry.last_used.get(), key))
            .collect();
        candidates.sort_unstable();
        let take = count.min(candidates.len());
        for (_, key) in candidates.into_iter().take(take) {
            self.entries.remove(&key);
        }
        take
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
/// A width change therefore costs a break. A colour change costs neither, because the brush is not
/// in either key. A `vertical-align` change costs a break, because the request carries the shift.
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
        assert_eq!(cache.evict_inactive(&[ParagraphKey(3)], 1), 1);
        assert!(cache.holds(ParagraphKey(1)));
        assert!(!cache.holds(ParagraphKey(2)));
        assert!(cache.holds(ParagraphKey(3)));

        assert_eq!(cache.evict_inactive(&[ParagraphKey(3)], usize::MAX), 1);
        assert_eq!(cache.len(), 1);
        assert!(cache.holds(ParagraphKey(3)));
    }
}
