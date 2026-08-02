//! Asking the offsetting walk for its two duties one at a time, and timing each.
//!
//! Moving a clean subtree does two unrelated things to every box in it. It moves the rectangles —
//! five per piece — and re-interns the clip chain a clipping box imposes on its contents; and it
//! tells the hit index and the accessibility layer where the pieces went. The first is arithmetic
//! over geometry this crate owns. The second maintains two structures whose readers are elsewhere,
//! and it would still have to be done by anything that replaced the arithmetic with a single
//! offset written once.
//!
//! Which of the two a moved document spends its time in is not a question a profile of the fused
//! walk answers. The two are interleaved per box and inlined into each other, so the symbol that
//! carries the time is the walk, and the walk is both of them.
//!
//! So the walk can be asked to make its descents separately: the bare traversal on its own, then
//! the geometry, then the index. Every duty is still discharged, in the same frame, in the same
//! order relative to the boxes it acts on — a frame measured this way is the same frame, made of
//! several descents instead of one. What each descent cost is accumulated here.
//!
//! ```
//! use zgui_layout::fragment::diff::split::{self, Passes};
//!
//! split::set(Passes::Apart);
//! assert_eq!(split::current(), Passes::Apart);
//! // A frame goes past here.
//! let spent = split::take();
//! assert_eq!(spent.walks, 0, "nothing was walked, so nothing was spent");
//! split::set(Passes::Together);
//! ```
//!
//! # What a caller has to do with the numbers
//!
//! Subtract. Each descent pays for the traversal it shares with the others — reading each box's
//! child list and recursing down it — so [`Spent::geometry`] is that traversal *plus* the
//! rectangles, and [`Spent::warmed`] is that traversal alone under the same conditions. The cost of
//! a duty is the difference. [`Spent::together`] is the fused walk over the same document, which is
//! what the parts have to add up to for the subtraction to mean anything.
//!
//! # What it cannot see
//!
//! Descents after the first read boxes the first one brought into the caches. That is why the
//! traversal is descended twice and both are reported: the difference between them is memory the
//! walk faults in, which is real and belongs to neither duty, and a duty measured against the
//! second is a duty measured with its share of that already paid. What the divided descents cannot
//! do is say how the faulting divides — a fused walk pays each stall once, and no arrangement of
//! separate descents can charge a single stall to two of them.
//!
//! It also cannot see inside a duty. A descent is timed as a whole, so the cost of interning a
//! clip chain and the cost of translating five rectangles arrive as one number.

use std::cell::Cell;
use std::time::Instant;

/// How the offsetting walk divides its duties.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Passes {
    /// One descent that does everything, timed by nobody. What every frame makes unless something
    /// has asked otherwise, and the only shape that costs a frame nothing at all.
    #[default]
    Together,
    /// One descent that does everything, timed as a whole.
    TogetherTimed,
    /// Four descents, each timed on its own: the traversal, the traversal again, the geometry, the
    /// index.
    ///
    /// Twice for the traversal because the first descent into a subtree brings its boxes back into
    /// the caches and the descents after it read them from there. Timing the traversal twice is
    /// what says how large that is: the difference between the two is memory the walk faults in,
    /// which belongs to neither duty, and the second is the traversal the other two are compared
    /// against.
    Apart,
}

/// Nanoseconds the offsetting walk spent, by descent, since the last [`take`].
///
/// Every field is a sum over every moved subtree of every frame since the last read, so a caller
/// that wants a frame's figure takes the accumulator at the end of each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spent {
    /// The fused descent: everything, in one pass over the subtree.
    pub together: u64,
    /// The traversal alone, doing nothing to any box it reaches, over boxes the walk has not
    /// touched this frame.
    pub skeleton: u64,
    /// The same traversal immediately again, over the boxes it has just brought in.
    pub warmed: u64,
    /// The traversal plus the rectangles and the clip chains.
    pub geometry: u64,
    /// The traversal plus the hit entries and the accessibility marks.
    pub index: u64,
    /// Bringing the hit index's hierarchy up to date over every entry the walk carried.
    ///
    /// Not a descent and not inside one. It is the other half of moving an entry — the walk writes
    /// each entry where it now is and leaves the structure above them for one pass at the end of
    /// the frame — so a reckoning of what telling the index costs that left it out would be a
    /// reckoning of half of it.
    pub settle: u64,
    /// How many subtrees were moved, counted once however many descents each one took.
    pub walks: u64,
}

/// Which descent a measurement belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// The fused descent.
    Together,
    /// The traversal alone, first.
    Skeleton,
    /// The traversal alone, again.
    Warmed,
    /// The rectangles and the clip chains.
    Geometry,
    /// The hit entries and the accessibility marks.
    Index,
    /// The hit index's hierarchy, brought up to date once at the end of the walk.
    Settle,
}

thread_local! {
    /// How the walk on this thread divides its duties.
    static PASSES: Cell<Passes> = const { Cell::new(Passes::Together) };
    /// What it has spent since the last read.
    static SPENT: Cell<Spent> = const { Cell::new(Spent {
        together: 0,
        skeleton: 0,
        warmed: 0,
        geometry: 0,
        index: 0,
        settle: 0,
        walks: 0,
    }) };
}

/// Asks the walks on this thread to divide their duties this way from now on.
pub fn set(passes: Passes) {
    PASSES.with(|cell| cell.set(passes));
}

/// How they are dividing them.
#[must_use]
pub fn current() -> Passes {
    PASSES.with(Cell::get)
}

/// Everything spent since this was last called, and resets the accumulator.
#[must_use]
pub fn take() -> Spent {
    SPENT.with(|cell| cell.replace(Spent::default()))
}

/// Runs one descent and adds what it cost to `part`.
pub(crate) fn timed<T>(part: Part, descend: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let out = descend();
    let nanos = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    SPENT.with(|cell| {
        let mut spent = cell.get();
        match part {
            Part::Together => spent.together += nanos,
            Part::Skeleton => spent.skeleton += nanos,
            Part::Warmed => spent.warmed += nanos,
            Part::Geometry => spent.geometry += nanos,
            Part::Index => spent.index += nanos,
            Part::Settle => spent.settle += nanos,
        }
        cell.set(spent);
    });
    out
}

/// Records that one subtree was moved, however many descents that took.
pub(super) fn walked() {
    SPENT.with(|cell| {
        let mut spent = cell.get();
        spent.walks += 1;
        cell.set(spent);
    });
}

#[cfg(test)]
mod tests;
