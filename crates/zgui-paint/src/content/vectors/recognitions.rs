//! What recognition found in a path, kept between frames by the identity of the path.
//!
//! A drawing that does not change hands its shapes back as the same allocations every frame, so
//! the address of a path names its geometry for as long as an entry holds its allocation. The
//! analytic route and the marks route both ask, the second with a larger limit, and a decline is
//! kept as well as a result: a path that is no recognised shape costs one attempt.
//!
//! A drawing placed again every frame hands back new allocations every frame, and an entry for
//! one of them is never asked for again. Holding those paths costs more than recognising them,
//! so an entry nothing has asked for again lives one frame only, and a frame that found nothing
//! it held stops adding entries until a later frame tries again.
//!
//! No result depends on an entry: a path with none is recognised again. So a full map drops the
//! entries touched least recently, and when a mark payload pins every entry, it keeps nothing new
//! until a pin drops.

use core::hash::Hash;
use std::sync::{Arc, Weak};

use rustc_hash::FxHashMap;
use zgui_profile::{Counter, counter};
use zgui_scene::kurbo::{self, BezPath};

use crate::emit::vector::recognise::Decomposition;

/// How many frames an entry survives without a lookup, once a lookup has found it.
const KEPT_FRAMES: u32 = 8;

/// How many frames an entry nothing found yet survives: the next frame may find it.
const UNPROVEN_FRAMES: u32 = 1;

/// How many misses with no hit stop a frame's entries from being kept.
const IDLE_MISSES: u32 = 16;

/// How often, in frames, the cache keeps entries while it is idle, to find out whether the
/// drawings have stopped changing.
const PROBE_FRAMES: u32 = 8;

/// The most entries held. A full map drops [`EVICTED`] unpinned entries, those touched least
/// recently first.
pub(super) const MAX_ENTRIES: usize = 4096;

/// How many entries a full map drops at once, so one scan for the oldest serves this many inserts.
const EVICTED: usize = MAX_ENTRIES / 16;

/// Which part of a shape a recognition was of: the interior, or the outline of one stroke style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PartKey {
    /// The interior.
    Fill,
    /// The outline of a stroke with this width, joins, caps and miter limit.
    Stroke {
        /// The width, as bits.
        width: u64,
        /// The join.
        join: u8,
        /// The cap at each start.
        start: u8,
        /// The cap at each end.
        end: u8,
        /// The miter limit, as bits.
        miter: u64,
    },
}

impl PartKey {
    /// The key of the outline `style` draws.
    pub(crate) fn stroke(style: &kurbo::Stroke) -> Self {
        let cap = |cap: kurbo::Cap| cap as u8;
        Self::Stroke {
            width: style.width.to_bits(),
            join: style.join as u8,
            start: cap(style.start_cap),
            end: cap(style.end_cap),
            miter: style.miter_limit.to_bits(),
        }
    }
}

/// One path's recognition.
#[derive(Debug)]
struct Entry {
    /// The path's allocation, held so no other path takes its address while the entry stands.
    ///
    /// Weak: the elements are freed with the last drawing that holds them, and only the small
    /// allocation that names them stays.
    _path: Weak<BezPath>,
    /// The power of two the tolerance was rounded down to.
    class: i32,
    /// The most primitives the recognition was allowed.
    max_prims: usize,
    /// What it found, or `None` for a decline.
    outcome: Option<Arc<Decomposition>>,
    /// The frame it was last looked up in.
    touched: u32,
    /// Whether a lookup has found it.
    proven: bool,
}

impl Entry {
    /// Whether something besides the entry holds its result: a mark payload lowered from it is
    /// still drawn.
    fn pinned(&self) -> bool {
        self.outcome
            .as_ref()
            .is_some_and(|found| Arc::strong_count(found) > 1)
    }
}

/// Recognitions of paths, by the address of the path and the part recognised.
#[doc(hidden)]
#[derive(Debug)]
pub struct Recognitions {
    /// The entries.
    entries: FxHashMap<(usize, PartKey), Entry>,
    /// The current frame.
    frame: u32,
    /// Lookups this frame that found an entry.
    hits: u32,
    /// Lookups this frame that found none.
    misses: u32,
    /// Whether this frame keeps what it recognises.
    keeping: bool,
    /// Whether the map was full with every entry pinned this frame.
    all_pinned: bool,
}

impl Default for Recognitions {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            frame: 0,
            hits: 0,
            misses: 0,
            keeping: true,
            all_pinned: false,
        }
    }
}

impl Recognitions {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.hits = 0;
        self.misses = 0;
    }

    /// Forgets every entry no lookup touched for [`KEPT_FRAMES`] frames, and every entry no lookup
    /// found within [`UNPROVEN_FRAMES`] frames, and decides whether the next frame keeps entries.
    ///
    /// A result something else holds stays: a mark payload lowered from it is still drawn.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.all_pinned = false;
        self.entries.retain(|_, entry| {
            if entry.pinned() {
                return true;
            }
            let kept = if entry.proven {
                KEPT_FRAMES
            } else {
                UNPROVEN_FRAMES
            };
            frame.wrapping_sub(entry.touched) < kept
        });
        let idle = self.hits == 0 && self.misses >= IDLE_MISSES;
        self.keeping = !idle || frame.wrapping_add(1).is_multiple_of(PROBE_FRAMES);
    }

    /// What `path` was found to be at tolerance `class` and limit `max_prims`, or `None` when no
    /// held entry answers that.
    ///
    /// A result answers any limit: under a lower limit than its count it is a decline. A decline
    /// answers its own limit and every lower one.
    pub(crate) fn lookup(
        &mut self,
        path: &Arc<BezPath>,
        part: PartKey,
        class: i32,
        max_prims: usize,
    ) -> Option<Option<Arc<Decomposition>>> {
        let answer = self
            .entries
            .get_mut(&(key(path), part))
            .filter(|entry| entry.class == class)
            .and_then(|entry| {
                let answer = match &entry.outcome {
                    Some(found) => Some((found.count <= max_prims).then(|| Arc::clone(found))),
                    None => (entry.max_prims >= max_prims).then_some(None),
                };
                if answer.is_some() {
                    entry.touched = self.frame;
                    entry.proven = true;
                }
                answer
            });
        if answer.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        answer
    }

    /// Keeps what `path` was found to be.
    pub(crate) fn insert(
        &mut self,
        path: &Arc<BezPath>,
        part: PartKey,
        class: i32,
        max_prims: usize,
        outcome: Option<Arc<Decomposition>>,
    ) {
        if !self.keeping {
            return;
        }
        let key = (key(path), part);
        if self.entries.len() >= MAX_ENTRIES && !self.entries.contains_key(&key) {
            self.all_pinned = self.all_pinned
                || !evict(&mut self.entries, self.frame, EVICTED, |entry| {
                    (entry.touched, entry.pinned())
                });
            if self.all_pinned {
                counter::bump(Counter::MarksUncached);
                return;
            }
        }
        self.entries.insert(
            key,
            Entry {
                _path: Arc::downgrade(path),
                class,
                max_prims,
                outcome,
                touched: self.frame,
                proven: false,
            },
        );
    }

    /// How many paths are held.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Forgets everything.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

/// The address a path is known by.
fn key(path: &Arc<BezPath>) -> usize {
    Arc::as_ptr(path).addr()
}

/// Drops up to `count` entries of `map` that are not pinned, those touched least recently first,
/// and returns whether it dropped any. `state` gives the frame an entry was last touched in and
/// whether it is pinned.
pub(crate) fn evict<K: Copy + Eq + Hash, V>(
    map: &mut FxHashMap<K, V>,
    frame: u32,
    count: usize,
    state: impl Fn(&V) -> (u32, bool),
) -> bool {
    let mut free: Vec<(u32, K)> = map
        .iter()
        .filter_map(|(key, entry)| {
            let (touched, pinned) = state(entry);
            (!pinned).then(|| (frame.wrapping_sub(touched), *key))
        })
        .collect();
    if free.len() > count {
        free.select_nth_unstable_by(count.max(1) - 1, |one, two| two.0.cmp(&one.0));
        free.truncate(count.max(1));
    }
    for (_, key) in &free {
        map.remove(key);
    }
    !free.is_empty()
}
