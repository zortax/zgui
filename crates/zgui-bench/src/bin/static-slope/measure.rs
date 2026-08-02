//! Driving the two updates and timing them.

use std::time::Instant;

use zgui::prelude::{Get, Set};
use zgui::runtime::Runtime;
use zgui_bench::reference::sample;
use zgui_platform_headless::Harness;

use super::document::Signals;

/// Flips `signal` and drives the loop until it goes quiet.
///
/// The flip and the settle are both inside the clock. A settle that ran outside it would time the
/// signal write and nothing else, and the signal write is the one part of an update that costs the
/// same in every document.
fn flip(
    harness: &mut Harness<Runtime>,
    signal: zgui::reactive::RwSignal<bool, zgui::reactive::LocalStorage>,
) -> std::time::Duration {
    let started = Instant::now();
    signal.set(!signal.get());
    harness.settle(256);
    started.elapsed()
}

/// The median cost of one single-property update on one control, in nanoseconds.
///
/// This is the click number: what a purely local change costs. Every size runs the same flip on the
/// same control, so what the slope through the four sizes measures is the part of that cost which
/// grew with the document — which for a change reaching one element ought to be nothing.
pub(crate) fn local(harness: &mut Harness<Runtime>, signals: Signals) -> f64 {
    sample::median_ns(|_| flip(harness, signals.hot))
}

/// The median cost of the same declaration changing on *every* control, in nanoseconds.
///
/// The same-run baseline. One class on the root, one property, and a document's worth of elements
/// whose rule sits under it — so this is the cost of a single-property update that genuinely has to
/// reach everything, measured on the same documents, in the same process, minutes apart at most.
pub(crate) fn global(harness: &mut Harness<Runtime>, signals: Signals) -> f64 {
    sample::median_ns(|_| flip(harness, signals.warm))
}

/// What one local update costs in work rather than in time.
///
/// A count is the same on every machine, so none of these needs a baseline or a tolerance: each is
/// either a function of the document or it is not. They are recorded together because the
/// interesting answer is which of them moves — a workload that reports only the restyle count can
/// say "the invalidation is local" while every stage downstream of it walks the whole document, and
/// that is precisely the shape the time on its own could not name.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Work {
    /// Elements whose selector matching ran again.
    pub(crate) restyled: u64,
    /// Nodes a phase traversal looked at, whether or not they owed any work.
    pub(crate) visited: u64,
    /// Nodes whose size or position was computed again.
    pub(crate) relaid_out: u64,
    /// Fragments compared against their previous geometry.
    pub(crate) diffed: u64,
    /// Primitives added to the scene.
    pub(crate) emitted: u64,
    /// Insertions into the structure that assigns draw order.
    pub(crate) inserts: u64,
}

/// What one local update costs, averaged over a handful of flips.
///
/// Averaged rather than taken from one, because the first flip after a batch of timed ones can find
/// work the last of them left owing.
pub(crate) fn work_per_local_update(harness: &mut Harness<Runtime>, signals: Signals) -> Work {
    /// How many flips the counts are averaged over.
    const FLIPS: u64 = 8;

    // Settle first, so nothing the timed passes left owing is charged to the counts.
    harness.settle(256);
    let before = zgui_profile::counter::snapshot();
    for _ in 0..FLIPS {
        flip(harness, signals.hot);
    }
    let moved = before.delta(&zgui_profile::counter::snapshot());
    Work {
        restyled: moved.elements_restyled / FLIPS,
        visited: moved.nodes_visited / FLIPS,
        relaid_out: moved.nodes_relaid_out / FLIPS,
        diffed: moved.fragments_diffed / FLIPS,
        emitted: moved.primitives_emitted / FLIPS,
        inserts: moved.bounds_tree_inserts / FLIPS,
    }
}
