//! The levels the entry-counted caches are held to, and what each number is derived from.
//!
//! Two of the five registered caches are budgeted in entries, and both numbers are stated here
//! rather than at their use so that the derivation is in one place and can be argued with. The
//! byte-counted caches are not here: the glyph atlas's level is
//! [`ATLAS_SOFT_BYTES`](crate::window::ATLAS_SOFT_BYTES) and belongs beside the window that installs
//! it, and the other two state no level at all — see their adapters for why.
//!
//! # Where the numbers come from
//!
//! Both are derived from the two workloads `zgui-bench` runs that bound this from either side.
//!
//! The **largest document held entirely live** is its still table: eight hundred and thirty-three
//! rows of six elements, about five thousand nodes, every one of them in the tree at once. A level
//! below that would make a document that is entirely on screen evict content it is drawing, which
//! is the one thing a soft level must never do.
//!
//! The **most distinct content a run leaves behind** is its scroll list: ten thousand rows, of
//! which the document holds a screenful and the caches accumulate every row that has passed. That
//! is the shape a level exists for at all — a cache bounded by the live tree cannot grow past it,
//! and this one is not.
//!
//! Sixteen thousand three hundred and eighty-four is above both, so no workload this project runs
//! evicts. It is *not* a measured capacity and does not claim to be: what is measured is the two
//! workloads, and the margin is a judgement that something past both of them should be bounded
//! rather than unbounded.

/// How many shaped paragraphs a window holds before inactive shaping is evicted.
///
/// Above both workloads the module note derives it from. A virtualized list is what this exists
/// for: its rows are recycled, so the *document* never holds more than a screenful, but every
/// distinct string that has scrolled past leaves a shaped result behind under its own key — ten
/// thousand rows leave ten thousand of them, and a hundred thousand leave a hundred thousand.
///
/// Current inline resolutions pin the entries they name. Crossing the limit therefore removes old
/// text versions and content no longer present, coldest first, without invalidating the live box
/// tree. A document whose active text alone exceeds the limit remains over it rather than being
/// made to reshape on every frame.
pub const SHAPED_PARAGRAPHS: usize = 16_384;

/// How many placed drawings a window holds before the vector cache is dropped.
///
/// The same derivation, and far cheaper to reach: a placed drawing is produced again by parsing the
/// notation on the element and fitting it to the box it is drawn into, and nothing measured from it
/// is invalidated, so the frame after the level fires places again whatever it draws and no more.
///
/// Most documents never approach it. The cache already drops every entry whose node has left the
/// document, once per frame, so what remains is bounded by the live tree — this bounds the case that
/// bound does not reach, which is a document that genuinely holds tens of thousands of live drawing
/// nodes.
pub const PLACED_DRAWINGS: usize = 16_384;

/// The levels one window holds its entry-counted caches to.
///
/// Settable rather than constant, for the same reason the atlas's level is: how much of the
/// machine's memory one window may have is not a question the window can answer on its own, and a
/// test that had to produce sixteen thousand paragraphs to observe a limit being enforced would be
/// asserting the limit's arithmetic rather than its enforcement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheLimits {
    /// How many shaped paragraphs may be held.
    pub shaped_paragraphs: usize,
    /// How many placed drawings may be held.
    pub placed_drawings: usize,
}

impl Default for CacheLimits {
    /// The levels stated in this module.
    fn default() -> Self {
        Self {
            shaped_paragraphs: SHAPED_PARAGRAPHS,
            placed_drawings: PLACED_DRAWINGS,
        }
    }
}
