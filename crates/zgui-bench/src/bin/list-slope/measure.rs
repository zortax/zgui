//! The two sweeps, and what a tick of a scroll damages and rebuilds.

use std::time::Instant;

use zgui::prelude::{Get, Set};
use zgui_bench::reference::{sample, watch};

use super::document::{Opened, opened};
use super::gesture::{self, Gesture};

/// What one pass of a gesture cost, and how many frames it drew.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pass {
    /// The median cost of one whole pass, in nanoseconds.
    pub(crate) cost: f64,
    /// How many frames one pass draws.
    ///
    /// Counted rather than assumed from the tick count, because a tick that damaged nothing draws
    /// no frame at all: dividing by ticks asked for rather than frames drawn would report a
    /// per-frame cost the surface never showed. It is what turns a per-pass cost into the per-frame
    /// number the glide baseline is stated in.
    pub(crate) frames: usize,
}

impl Pass {
    /// The cost of one drawn frame of the pass, in nanoseconds.
    #[expect(clippy::cast_precision_loss, reason = "a frame count is in the tens")]
    pub(crate) fn per_frame(self) -> Option<f64> {
        (self.frames > 0).then(|| self.cost / self.frames as f64)
    }
}

/// The median cost of one pass of `gesture` over an already-open list.
pub(crate) fn gesture_ns(open: &mut Opened, height: f32, gesture: Gesture) -> Pass {
    let at = gesture::middle(height);
    gesture::aim(&mut open.harness, at);
    let cost = sample::median_ns(|turn| gesture.drive(&mut open.harness, at, turn));
    open.damage.borrow_mut().clear();
    gesture.drive(&mut open.harness, at, 0);
    let frames = open.damage.borrow().len();
    Pass { cost, frames }
}

/// The median cost of repainting every realised row without scrolling, in nanoseconds.
///
/// The same-run baseline of the glide slope. One class on the list that every realised row's colour
/// sits under, so it restyles and repaints exactly the rows a glide moves — the same set, in the
/// same document, in the same process — by a route that has nothing to do with scrolling. The
/// dimensionless thing the gate compares is the ratio of the two slopes, which is what makes it the
/// same number on a laptop and on a loaded CI host.
pub(crate) fn repaint_ns(open: &mut Opened) -> f64 {
    sample::median_ns(|_| {
        let started = Instant::now();
        open.repaint.set(!open.repaint.get());
        open.harness.settle(256);
        started.elapsed()
    })
}

/// What one tick of a gesture costs in damage and in rebuilt fragments.
///
/// Both are already dimensionless — a share of a surface, a count per frame — so neither needs a
/// baseline and neither is keyed to a machine.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PerTick {
    /// The mean share of the surface a drawn frame damaged, or `None` when nothing was drawn.
    pub(crate) damage: Option<f64>,
    /// How many of the drawn frames declared the whole surface damaged, as a number to judge.
    pub(crate) full_frames: Option<f64>,
    /// Fragments whose geometry was recomputed, per frame drawn.
    pub(crate) fragments_rebuilt: Option<f64>,
    /// How many frames the gesture actually drew.
    pub(crate) frames: usize,
    /// How many of them declared the whole surface damaged.
    pub(crate) full: usize,
}

/// Drives `gesture` once over a settled list and reports what its ticks did.
///
/// One pass rather than the forty-eight a time is a median of: a count and a fraction are not
/// samples of a distribution the scheduler perturbs, and forty-eight passes would leave the list
/// somewhere the first pass never reached.
#[expect(
    clippy::cast_precision_loss,
    reason = "a frame count is in the tens and a fragment count in the thousands"
)]
pub(crate) fn per_tick(open: &mut Opened, height: f32, gesture: Gesture) -> PerTick {
    let at = gesture::middle(height);
    gesture::aim(&mut open.harness, at);
    // A warm pass first, so the frames counted are a gesture over a list that has already been
    // scrolled once — which is what every gesture but the first is.
    gesture.drive(&mut open.harness, at, 0);
    open.harness.settle(256);
    open.damage.borrow_mut().clear();
    let before = zgui_profile::counter::snapshot();
    gesture.drive(&mut open.harness, at, 1);
    let moved = before.delta(&zgui_profile::counter::snapshot());
    let frames = open.damage.borrow().len();
    let full = watch::full_frames(&open.damage);
    PerTick {
        damage: watch::mean_fraction(&open.damage),
        full_frames: (frames > 0).then_some(full as f64),
        fragments_rebuilt: (frames > 0).then(|| moved.fragments_rebuilt as f64 / frames as f64),
        frames,
        full,
    }
}

/// One point of a sweep: the document it was taken on and what the gesture cost there.
pub(crate) struct Point {
    /// How many rows the model held.
    pub(crate) rows: usize,
    /// How many rows the list actually built.
    pub(crate) built: usize,
    /// The median cost of one pass, in nanoseconds.
    pub(crate) cost: f64,
}

/// Opens a list, drives `gesture` over it, and closes it.
pub(crate) fn at(rows: usize, height: f32, gesture: Gesture) -> Point {
    let mut open = opened(rows, height);
    let built = super::document::rows_built(&open.harness);
    assert!(
        built > 0,
        "a list of {rows} rows in a {height}px port built no rows at all, so this sweep is \
         measuring an empty document",
    );
    let pass = gesture_ns(&mut open, height, gesture);
    Point {
        rows,
        built,
        cost: pass.cost,
    }
}
