//! What recognition found in a path, kept between frames by the identity of the path.
//!
//! A drawing that does not change hands its shapes back as the same allocations every frame, so
//! the address of a path names its geometry for as long as an entry holds the path alive. The
//! analytic route and the marks route both ask, the second with a larger limit, and a decline is
//! kept as well as a result: a path that is no recognised shape costs one attempt.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use zgui_scene::kurbo::{self, BezPath};

use crate::emit::vector::recognise::Decomposition;

/// How many frames an entry survives without a lookup.
const KEPT_FRAMES: u32 = 8;

/// The most entries held. A full map keeps what it holds and adds nothing.
const MAX_ENTRIES: usize = 4096;

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
    /// The path, held so its address names it while the entry stands.
    _path: Arc<BezPath>,
    /// The power of two the tolerance was rounded down to.
    class: i32,
    /// The most primitives the recognition was allowed.
    max_prims: usize,
    /// What it found, or `None` for a decline.
    outcome: Option<Arc<Decomposition>>,
    /// The frame it was last looked up in.
    touched: u32,
}

/// Recognitions of paths, by the address of the path and the part recognised.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct Recognitions {
    /// The entries.
    entries: FxHashMap<(usize, PartKey), Entry>,
    /// The current frame.
    frame: u32,
}

impl Recognitions {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Forgets every entry no lookup touched for [`KEPT_FRAMES`] frames.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.entries
            .retain(|_, entry| frame.wrapping_sub(entry.touched) < KEPT_FRAMES);
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
        let entry = self.entries.get_mut(&(key(path), part))?;
        if entry.class != class {
            return None;
        }
        let answer = match &entry.outcome {
            Some(found) => Some((found.count <= max_prims).then(|| Arc::clone(found))),
            None => (entry.max_prims >= max_prims).then_some(None),
        };
        if answer.is_some() {
            entry.touched = self.frame;
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
        let key = (key(path), part);
        if self.entries.len() >= MAX_ENTRIES && !self.entries.contains_key(&key) {
            return;
        }
        self.entries.insert(
            key,
            Entry {
                _path: Arc::clone(path),
                class,
                max_prims,
                outcome,
                touched: self.frame,
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
