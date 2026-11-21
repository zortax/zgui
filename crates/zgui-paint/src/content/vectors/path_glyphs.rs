//! The splits of the paths drawn lately, and the outlines each part of a split is drawn as.
//!
//! A split is kept by the identity of the path and the device map, with the payload lowered from
//! each part, so a path that does not change splits once and lowers once.

use core::hash::{Hash, Hasher};
use std::sync::{Arc, Weak};

use compositor_types::MarkPayload;
use kurbo::{self, BezPath, Point, Shape as _};
use rustc_hash::{FxHashMap, FxHasher};

use super::recognitions::evict;
use crate::emit::vector::split::{
    CLOSE, CUBIC, Geometry, LINE, MOVE, QUAD, Split, SplitDeclined, UNITS,
};

/// The widest and tallest an outline may be, in device pixels.
pub(crate) const MAX_CELL: f64 = 64.0;

/// How far a stroked outline may lie from the true stroke, in device pixels.
const TOLERANCE: f64 = 1.0 / 64.0;

/// How one part's outlines are produced.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PartStyle<'a> {
    /// The outlines are filled.
    Fill,
    /// The outlines are stroked, with lengths in units `scale` maps to device pixels.
    Stroke {
        /// The stroke.
        style: &'a kurbo::Stroke,
        /// Device pixels per stroke unit.
        scale: f64,
    },
}

impl PartStyle<'_> {
    /// Returns a number equal styles share.
    pub(crate) fn key(self) -> u64 {
        let mut hasher = FxHasher::default();
        match self {
            Self::Fill => 0u8.hash(&mut hasher),
            Self::Stroke { style, scale } => {
                1u8.hash(&mut hasher);
                for value in [
                    style.width * scale,
                    style.miter_limit,
                    style.dash_offset * scale,
                ]
                .into_iter()
                .chain(style.dash_pattern.iter().map(|dash| dash * scale))
                {
                    value.to_bits().hash(&mut hasher);
                }
                (style.join as u8, style.start_cap as u8, style.end_cap as u8).hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}

/// The outlines of one part, filled with the nonzero rule, in device pixels from their anchors.
#[derive(Clone, Debug)]
pub(crate) struct Outlines {
    /// One outline per geometry.
    pub(crate) shapes: Vec<Arc<BezPath>>,
    /// The control box of each outline, as `[x0, y0, x1, y1]`.
    pub(crate) reach: Arc<[[f64; 4]]>,
}

/// Returns the outlines `geometries` are drawn as in `style`, or `None` when one is wider or
/// taller than [`MAX_CELL`] or a stroke has no width.
pub(crate) fn outlines(geometries: &[Geometry], style: PartStyle<'_>) -> Option<Outlines> {
    let scaled = match style {
        PartStyle::Fill => None,
        PartStyle::Stroke { style, scale } => {
            let mut scaled = style.clone();
            scaled.width *= scale;
            scaled.dash_offset *= scale;
            for dash in scaled.dash_pattern.iter_mut() {
                *dash *= scale;
            }
            if !(scaled.width.is_finite() && scaled.width > 0.0) {
                return None;
            }
            Some(scaled)
        }
    };
    let mut shapes = Vec::with_capacity(geometries.len());
    let mut reach = Vec::with_capacity(geometries.len());
    for geometry in geometries {
        let outline = path_of(&geometry.commands);
        let outline = match &scaled {
            Some(stroke) => kurbo::stroke(
                outline.iter(),
                stroke,
                &kurbo::StrokeOpts::default(),
                TOLERANCE,
            ),
            None => outline,
        };
        let bounds = outline.bounding_box();
        if !(bounds.width() <= MAX_CELL && bounds.height() <= MAX_CELL) {
            return None;
        }
        reach.push([bounds.x0, bounds.y0, bounds.x1, bounds.y1]);
        shapes.push(Arc::new(outline));
    }
    Some(Outlines {
        shapes,
        reach: Arc::from(reach),
    })
}

/// Returns the outline the words of a geometry describe, in device pixels from its anchor.
fn path_of(words: &[i32]) -> BezPath {
    let point = |at: usize| {
        Point::new(
            f64::from(words[at]) / UNITS,
            f64::from(words[at + 1]) / UNITS,
        )
    };
    let mut path = BezPath::new();
    let mut at = 0;
    while let Some(&tag) = words.get(at) {
        match tag {
            MOVE => path.move_to(point(at + 1)),
            LINE => path.line_to(point(at + 1)),
            QUAD => path.quad_to(point(at + 1), point(at + 3)),
            CUBIC => path.curve_to(point(at + 1), point(at + 3), point(at + 5)),
            CLOSE => path.close_path(),
            _ => {}
        }
        at += 1 + match tag {
            MOVE | LINE => 2,
            QUAD => 4,
            CUBIC => 6,
            _ => 0,
        };
    }
    path
}

/// How many frames an entry survives without a lookup, once a lookup has found it.
const KEPT_FRAMES: u32 = 8;

/// How many frames an entry nothing found yet survives.
const UNPROVEN_FRAMES: u32 = 1;

/// How many misses with no hit stop a frame's entries from being kept.
const IDLE_MISSES: u32 = 16;

/// How often, in frames, the cache keeps entries while it is idle.
const PROBE_FRAMES: u32 = 8;

/// The most entries held. A full map drops [`EVICTED_SPLITS`] entries, those touched least
/// recently first.
pub(crate) const MAX_SPLITS: usize = 256;

/// How many entries a full map drops at once, so one scan for the oldest serves this many inserts.
const EVICTED_SPLITS: usize = MAX_SPLITS / 16;

/// One part's payload, with the style its outlines were built in.
#[derive(Debug)]
pub(crate) struct PartPayload {
    /// The style the outlines were built in, as [`PartStyle::key`] gives it.
    pub(crate) style: u64,
    /// Whether the part's copies overlap and are drawn as one union.
    pub(crate) union: bool,
    /// The control box of each outline from its anchor, as `[x0, y0, x1, y1]`.
    pub(crate) reach: Arc<[[f64; 4]]>,
    /// The outlines and the anchors.
    pub(crate) payload: Arc<MarkPayload>,
}

/// One path's split.
#[derive(Debug)]
pub(crate) struct SplitEntry {
    /// The path's allocation, held so no other path takes its address while the entry stands.
    path: Weak<BezPath>,
    /// What the split found, or why it declined.
    pub(crate) outcome: Result<Arc<Split>, SplitDeclined>,
    /// The payload of each part, fill and stroke, lowered from the split.
    pub(crate) payloads: [Option<PartPayload>; 2],
    /// The frame it was last looked up in.
    touched: u32,
    /// Whether a lookup has found it.
    proven: bool,
}

/// Splits of paths, by the address of the path and the bits of the device map.
#[doc(hidden)]
#[derive(Debug)]
pub struct Splits {
    /// The entries.
    entries: FxHashMap<(usize, [u64; 4]), SplitEntry>,
    /// The current frame.
    frame: u32,
    /// Lookups this frame that found an entry.
    hits: u32,
    /// Lookups this frame that found none.
    misses: u32,
    /// Whether this frame keeps what it splits.
    keeping: bool,
}

impl Default for Splits {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            frame: 0,
            hits: 0,
            misses: 0,
            keeping: true,
        }
    }
}

impl Splits {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.hits = 0;
        self.misses = 0;
    }

    /// Forgets every entry whose path died, every entry no lookup touched for [`KEPT_FRAMES`]
    /// frames, and every entry no lookup found within [`UNPROVEN_FRAMES`] frames, and decides
    /// whether the next frame keeps entries.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.entries.retain(|_, entry| {
            if entry.path.strong_count() == 0 {
                return false;
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

    /// The entry of `path` under `linear`, if one is held.
    pub(crate) fn lookup(
        &mut self,
        path: &Arc<BezPath>,
        linear: [f64; 4],
    ) -> Option<&mut SplitEntry> {
        let frame = self.frame;
        match self.entries.get_mut(&key(path, linear)) {
            Some(entry) => {
                entry.touched = frame;
                entry.proven = true;
                self.hits += 1;
                Some(entry)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// The entry of `path` under `linear`, if one is held, for its payloads.
    ///
    /// No lookup: the entry is not touched or proven, and nothing counts as a hit or a miss. A
    /// path split this frame keeps its payloads here, and lives on only if a later frame finds it.
    pub(crate) fn held(
        &mut self,
        path: &Arc<BezPath>,
        linear: [f64; 4],
    ) -> Option<&mut SplitEntry> {
        self.entries.get_mut(&key(path, linear))
    }

    /// Keeps what `path` split into under `linear`, and returns the entry, or `None` when this
    /// frame keeps nothing.
    pub(crate) fn insert(
        &mut self,
        path: &Arc<BezPath>,
        linear: [f64; 4],
        outcome: Result<Arc<Split>, SplitDeclined>,
    ) -> Option<&mut SplitEntry> {
        if !self.keeping {
            return None;
        }
        let key = key(path, linear);
        if self.entries.len() >= MAX_SPLITS && !self.entries.contains_key(&key) {
            evict(&mut self.entries, self.frame, EVICTED_SPLITS, |entry| {
                (entry.touched, false)
            });
        }
        self.entries.insert(
            key,
            SplitEntry {
                path: Arc::downgrade(path),
                outcome,
                payloads: [None, None],
                touched: self.frame,
                proven: false,
            },
        );
        self.entries.get_mut(&key)
    }

    /// Forgets everything.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

/// The key of `path` under `linear`.
fn key(path: &Arc<BezPath>, linear: [f64; 4]) -> (usize, [u64; 4]) {
    (Arc::as_ptr(path).addr(), linear.map(f64::to_bits))
}

#[cfg(test)]
impl Splits {
    /// How many paths are held.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests;
