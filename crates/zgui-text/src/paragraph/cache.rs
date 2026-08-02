//! Holding shaped paragraphs across frames, and the protocol that uses them.

use core::cell::Cell;

use rustc_hash::FxHashMap;

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
    entries: FxHashMap<ParagraphKey, ShapedParagraph<E>>,
    /// How many lookups have found a held result since the cache was built.
    ///
    /// Monotonic and never reset, because the question it is kept for is asked between two moments
    /// rather than about one: a reader subtracts two readings to learn whether anything read the
    /// cache in between, and a per-frame figure would need the cache to be told when a frame is.
    hits: Cell<u64>,
}

impl<E> ParagraphCache<E> {
    /// An empty cache.
    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
            hits: Cell::new(0),
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
        if held.is_some() {
            self.hits.set(self.hits.get() + 1);
        }
        held
    }

    /// The result held under one key, for a breaking pass.
    ///
    /// Breaking is what a shaped result is *for*, and it mutates it: the glyphs record which break
    /// they currently reflect, which is what makes the next request with the same key free.
    pub fn get_mut(&mut self, key: ParagraphKey) -> Option<&mut ShapedParagraph<E>> {
        let held = self.entries.get_mut(&key);
        if held.is_some() {
            let hits = self.hits.get_mut();
            *hits += 1;
        }
        held
    }

    /// Holds one shaped result, under the key it carries.
    ///
    /// The key is the result's own rather than an argument, so a caller cannot file a paragraph
    /// under a key that does not describe it and serve it to a different one later.
    pub fn insert(&mut self, shaped: ShapedParagraph<E>) {
        self.entries.insert(shaped.key(), shaped);
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
    let shaped = cache
        .entries
        .entry(key)
        .or_insert_with(|| shaper.shape(content));
    let broken = shaper.break_lines(shaped, request);
    (shaped, broken)
}
