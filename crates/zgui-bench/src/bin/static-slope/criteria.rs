//! What this workload's numbers are allowed to be, and what they actually are.
//!
//! All three gates are one-sided. There is no value worth recording two-sidedly for any of them,
//! because for all three the ideal is zero and the failure is one: a change that reaches one control
//! should cost nothing per control in the document. Smaller is the win the workload exists to
//! protect, and a two-sided band around a number whose right answer is zero would fail an engine
//! that got better.
//!
//! # What this workload found, stated plainly
//!
//! A single-property update on one control of ten thousand restyles **one** element, lays out
//! **none**, diffs **six** fragments, emits **one** primitive and takes **one** draw-order
//! insertion — at every one of the four sizes. Every stage that performs work is already
//! independent of the document.
//!
//! It also **visits every node in the document**: `nodes_visited` reads the control count plus
//! seventeen at all four sizes, and that walk is what the seventy-odd nanoseconds per control in
//! the advisory line are. So the time ratio does not sit near zero; it sits near 0.068, and the
//! reason is one traversal rather than any stage doing real work it should not.
//!
//! That is a finding, not a failure of this workload, and it is not fixed here — C0's job is to
//! make the claim measurable, and the counter that names it is
//! [`NodesVisited`](zgui_profile::counter::Counter::NodesVisited), whose own documentation says it
//! exists for exactly this case. The gates below are therefore written against what the engine does
//! today, in a shape that gets *easier* to pass as the walk is narrowed and fails immediately if a
//! second one is added.

use zgui_bench::reference::verdict::{Allowed, Criterion};

/// The single-property update's slope, against the slope of the same property changing everywhere.
///
/// # Why the ceiling is where it is
///
/// Measured at 0.0679–0.0691 over five runs on one machine — a spread of about one per cent,
/// because both halves are medians of forty-eight samples taken minutes apart at most in one
/// process, so the machine's own drift is common to both and divides out. The ceiling is 0.09,
/// which is thirty per cent of headroom over the widest of those and still an order of magnitude
/// under the ratio a local update that had genuinely stopped being local would produce.
///
/// A ceiling rather than a band, because the number this bounds is expected to *fall*: the walk
/// underneath it is one traversal of a document that owes nothing, and a phase that narrows it
/// takes this ratio towards zero. A two-sided band recorded at 0.068 would fail that phase.
pub(crate) const LOCALITY: Criterion = Criterion {
    name: "STATIC-locality",
    subject: "a single-property update on one control of ten thousand",
    baseline: "the same property changing on every control in the document",
    allowed: Allowed::Under { most: 0.09 },
    advice: "A change reaching one control has got dearer against a change reaching all of them. \
             The counter lines above say where: if `restyled`, `relaid_out`, `diffed`, `emitted` \
             or `tree_inserts` has stopped being constant across the four sizes, an invalidation \
             widened. If they are all still constant and only `visited` grew, a second traversal \
             of the whole document was added to the update path.",
};

/// Elements restyled by a one-control update, per control in the document.
///
/// The same claim in counts rather than in time, and the stronger of the two: a count is a property
/// of the design and reads the same on a slow machine, a fast one and under a debugger, so it
/// cannot be quieted by a faster processor the way a time can.
///
/// **This reads exactly zero today** — one element restyled at 1 250 controls and one at 10 000 —
/// so the ceiling is not a value that was recorded, it is a bound on a slope whose right answer is
/// nothing. One extra restyle per thousand controls added is already a hundred times more than the
/// answer; the ceiling is loose because the number under it should not move at all, and a loose
/// ceiling nothing approaches fires only on a real change of shape.
pub(crate) const RESTYLE_LOCALITY: Criterion = Criterion {
    name: "STATIC-restyle-locality",
    subject: "elements restyled by a one-control update, per control in the document",
    baseline: "zero, which is what a local invalidation restyles per control it did not touch",
    allowed: Allowed::Under { most: 0.001 },
    advice: "The number of elements a one-control change restyles is growing with the document. \
             That is a selector-matching or invalidation change rather than a performance one: \
             find what made the changed element's restyle root wider than the element.",
};

/// Nodes a one-control update visits, per control in the document.
///
/// **This reads 1.0 today**, and that is the whole of the time ratio above: one pass over a
/// document that owes nothing, at every size. It is recorded as a gate anyway, and the gate is
/// worth having in exactly the shape it is in — a ceiling a little over one.
///
/// What it catches is a *second* walk. A phase that adds one takes this to two and fails on the
/// spot, which no timing gate on a fast machine would notice: visiting a clean node is tens of
/// nanoseconds, and doubling tens of nanoseconds ten thousand times is well inside the tolerance
/// any time-based band has to carry. What it must not be read as is approval of the walk it
/// records. The number a compositor phase that narrows the traversal produces is far below one, and
/// this ceiling passes it.
pub(crate) const VISIT_LOCALITY: Criterion = Criterion {
    name: "STATIC-visit-locality",
    subject: "nodes a one-control update visits, per control in the document",
    baseline: "one, which is the single whole-document traversal the update path runs today",
    allowed: Allowed::Under { most: 1.10 },
    advice: "The update path now walks the document more than once per change. Find the traversal \
             that was added; every stage that performs work on this path is already independent of \
             the document, so a second walk is pure overhead and nothing downstream of it will \
             show up in any other counter.",
};
