//! Driving one pass of the gesture with the walk divided one way or the other.

use std::time::{Duration, Instant};

use zgui_layout::fragment::diff::split::{self, Passes, Spent};
use zgui_profile::counter;

use super::document::{Opened, notch};

/// One tick of a 120 Hz refresh.
const TICK: Duration = Duration::from_micros(8_333);

/// How many ticks of glide one notch is carried for.
///
/// Short of the whole glide on purpose. What is being timed is the walk a moving frame makes, and
/// a tail of ticks moving a fraction of a pixel each contributes frames whose walk is the same size
/// as any other frame's while their number varies with how the spring happened to land.
const GLIDE_TICKS: usize = 16;

/// What one pass of the gesture cost, and what the offsetting walk did inside it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Drive {
    /// Wall-clock nanoseconds for the whole pass — the notch and every tick of the glide.
    pub(crate) wall: f64,
    /// What each descent of the offsetting walk spent, in nanoseconds.
    pub(crate) spent: Spent,
    /// Boxes the offsetting walk reached, counted once per box however many descents it made.
    pub(crate) visited: f64,
    /// Frames the pass drew.
    pub(crate) frames: usize,
}

impl Drive {
    /// The traversal every descent shares, in nanoseconds — measured, not inferred, and measured
    /// over boxes the descent before it has just brought into the caches, which is the state the
    /// two duties are measured in as well.
    pub(crate) fn traversal(self) -> f64 {
        nanos(self.spent.warmed)
    }

    /// What the walk faults in: the first traversal of a subtree against the same traversal again.
    ///
    /// It is a cost of moving the document and it belongs to neither duty, because a fused walk
    /// pays each stall once and no division of it can charge one stall to both halves.
    pub(crate) fn faulting(self) -> f64 {
        nanos(self.spent.skeleton) - nanos(self.spent.warmed)
    }

    /// The rectangles and the clip chains, with the traversal taken back off.
    pub(crate) fn geometry(self) -> f64 {
        nanos(self.spent.geometry) - self.traversal()
    }

    /// The hit entries and the accessibility marks, with the traversal taken back off.
    pub(crate) fn index(self) -> f64 {
        nanos(self.spent.index) - self.traversal()
    }

    /// Bringing the hit index's hierarchy up to date, once at the end of the walk.
    ///
    /// Part of what telling the index costs and not part of any descent: the walk writes each entry
    /// where it now is and leaves the structure above them for one pass at the end of the frame.
    pub(crate) fn settle(self) -> f64 {
        nanos(self.spent.settle)
    }

    /// Everything the index half costs: the descent that writes the entries and the pass that
    /// repairs the structure over them.
    pub(crate) fn telling(self) -> f64 {
        self.index() + self.settle()
    }

    /// The share of the two duties that is the index half, when both were measured.
    ///
    /// `None` when the pass made no divided descent, and when the two together came to nothing —
    /// a share of zero work is not a share of anything.
    pub(crate) fn index_share(self) -> Option<f64> {
        let both = self.geometry() + self.telling();
        (both > 0.0).then(|| self.telling() / both)
    }
}

/// A count of nanoseconds as a float, for arithmetic that is about to divide.
#[expect(
    clippy::cast_precision_loss,
    reason = "a pass is milliseconds, which is nowhere near the integers a double loses"
)]
pub(crate) fn nanos(count: u64) -> f64 {
    count as f64
}

/// Drives one notch and the glide it starts, with the walk dividing its duties `mode`'s way.
///
/// Every pass scrolls the same way. Reversing would be the more natural gesture and it makes the
/// measurement worse: the scroller starts at the top, so half the passes end against the limit,
/// where a notch moves the document less far, fewer frames move at all, and the three shapes of the
/// walk are no longer being asked to do the same amount of work. The document is long enough that a
/// whole run of one-way passes travels a fraction of it.
pub(crate) fn drive(open: &mut Opened, mode: Passes, _turn: usize) -> Drive {
    let lines = 6.0;
    split::set(mode);
    let _ = split::take();
    open.damage.borrow_mut().clear();
    let before = counter::snapshot();
    let started = Instant::now();
    open.harness.deliver_to_first(notch(lines));
    open.harness.settle(64);
    for _ in 0..GLIDE_TICKS {
        open.harness.advance(TICK);
        open.harness.pump();
    }
    let wall = started.elapsed();
    let spent = split::take();
    let moved = before.delta(&counter::snapshot());
    split::set(Passes::Together);
    Drive {
        wall: wall.as_secs_f64() * 1e9,
        spent,
        visited: nanos(moved.nodes_visited),
        frames: open.damage.borrow().len(),
    }
}
