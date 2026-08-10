//! The five end-to-end scenarios, and the ratchet around them.
//!
//! A scenario is one interaction driven over one document, and everything it reports carries a
//! [`Band`](band::Band). The gate runs all five, compares every number against its band and fails
//! on the first that is outside — so "performance is good" stops being a claim somebody made once
//! and becomes a thing the definition of done re-establishes on every run.
//!
//! # Why each scenario is its own process
//!
//! Two of the five would be measuring something else otherwise. **Cold start** is only cold once:
//! the second application built on a thread finds the font stack enumerated, the interned names
//! populated and the process's own pages faulted in, so a cold start measured after any other
//! scenario is a warm one wearing the name. And every scenario mounts an application through
//! thread-local state that a second mount adds to rather than replaces. So the runner spawns
//! itself once per scenario and reads the numbers back off its own output.

pub(crate) mod band;
mod cold;
mod fixture;
mod hover;
mod idle;
mod record;
mod scroll;

use zgui::runtime::Runtime;
use zgui_platform_headless::Harness;

use crate::scenario::band::{Measurement, Pace};
pub(crate) use crate::scenario::record::write;

mod kitchen;

/// Every scenario, in the order a report lists them.
pub(crate) const ALL: [&str; 5] = [
    "idle",
    "hover-storm",
    "scroll",
    "cold-start",
    "kitchen-sink",
];

/// What one scenario measured.
pub(crate) struct Outcome {
    /// Which scenario this is, as [`ALL`] spells it.
    pub(crate) scenario: &'static str,
    /// What it was driven over, in enough detail to reproduce the numbers.
    pub(crate) document: String,
    /// Every banded number it reports.
    pub(crate) measurements: Vec<Measurement>,
    /// The counters the scene-rebuild question is decided on.
    pub(crate) counters: Escalation,
    /// What this scenario found that no band expresses.
    ///
    /// A band answers "did this get worse". A note answers "what is this frame actually doing",
    /// which is the question a missed budget raises and the one a band can never answer, because
    /// the shape of the evidence differs per scenario: a row shift's rebuild count means nothing
    /// to a cold start. Notes are carried into the report verbatim and are how an escalation is
    /// opened with something attached to it.
    pub(crate) notes: Vec<String>,
    /// How its frames landed against the interval it drove them at.
    ///
    /// Every scenario has one, including the ones that are not about smoothness, because a late
    /// frame that only ever appears where somebody expected it is a late frame nobody found.
    pub(crate) pace: Pace,
}

/// The counters that turn "the scene rebuild is the dominant cost" into a measurement.
///
/// Recorded for every scenario whether or not that scenario is about emission, because the point of
/// them is comparison: a number that only exists for the scenario somebody suspected proves nothing
/// about the one they did not.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Escalation {
    /// How many primitives reached the display list.
    pub(crate) primitives_emitted: u64,
    /// How many were refused by a clip before they got there.
    pub(crate) primitives_culled: u64,
    /// How many insertions the draw-order tree took.
    pub(crate) bounds_tree_inserts: u64,
    /// How many recorded paintings were encoded from scratch.
    pub(crate) chunks_reencoded: u64,
    /// How many recorded paintings were replayed instead.
    pub(crate) chunks_translated: u64,
}

/// The stray work a translation frame is not allowed to do.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stray {
    /// Elements whose computed style was recomputed.
    pub(crate) restyles: u64,
    /// Nodes laid out again.
    pub(crate) relayouts: u64,
    /// Times the hit index was rebuilt from scratch.
    pub(crate) hit_index_rebuilds: u64,
}

/// Pulls the escalation counters out of a snapshot delta.
pub(crate) fn counters(moved: &zgui_profile::Counters) -> Escalation {
    Escalation {
        primitives_emitted: moved.primitives_emitted,
        primitives_culled: moved.primitives_culled,
        bounds_tree_inserts: moved.bounds_tree_inserts,
        chunks_reencoded: moved.chunks_reencoded,
        chunks_translated: moved.chunks_translated,
    }
}

/// The median of `samples`, which are sorted in place.
///
/// The median rather than the mean, because one sample of an interaction is occasionally the
/// scheduler's rather than the framework's and a mean carries that outlier into the comparison for
/// ever.
pub(crate) fn median(samples: &mut [f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

/// Drives `harness` until it stops asking for frames, so what follows is measured from rest.
///
/// A document that has just opened owes a first cascade, a first layout, a first shaping of every
/// run and a first emission of every primitive. Measuring an interaction before that has settled
/// measures the opening.
pub(crate) fn quiet(harness: &mut Harness<Runtime>) {
    harness.settle(256);
    for _ in 0..8 {
        harness.advance(std::time::Duration::from_micros(16_667));
        harness.pump();
    }
    harness.settle(256);
    zgui_profile::counter::reset();
}

/// Runs one scenario by name.
///
/// # Panics
///
/// Panics when `name` is not one of [`ALL`], because a runner that silently skipped an unknown
/// scenario would report a green sweep of four.
pub(crate) fn run(name: &str) -> Outcome {
    match name {
        "idle" => idle::run(),
        "hover-storm" => hover::run(),
        "scroll" => scroll::run(),
        "cold-start" => cold::run(),
        "kitchen-sink" => kitchen::run(),
        other => panic!("unknown scenario `{other}`; one of {ALL:?}"),
    }
}

/// Prints one scenario's result in the form the sweep parses back.
pub(crate) fn print(outcome: &Outcome) {
    println!("SCENARIO {} {}", outcome.scenario, outcome.document);
    for measurement in &outcome.measurements {
        println!(
            "MEASURE\t{}\t{}\t{}\t{:.4}\t{:.4}\t{}\t{}\t{}\t{}\t{}",
            outcome.scenario,
            measurement.name,
            measurement.unit,
            measurement.value,
            measurement.band.limit(),
            if measurement.passed() { "ok" } else { "over" },
            measurement.rationale,
            measurement
                .budget
                .map_or_else(|| "-".to_owned(), |budget| format!("{budget:.4}")),
            match measurement.met_budget() {
                Some(true) => "met",
                Some(false) => "missed",
                None => "-",
            },
            // The distribution, as one field so that a reader of the stored run can tell a
            // measurement that has no tail from one whose tail was never taken.
            measurement.spread.map_or_else(
                || "-".to_owned(),
                |spread| format!(
                    "p50={:.4};p95={:.4};p99={:.4};max={:.4};n={}",
                    spread.p50, spread.p95, spread.p99, spread.max, spread.samples
                )
            ),
        );
    }
    println!(
        "PACE\t{}\t{:.4}\t{}\t{}",
        outcome.scenario, outcome.pace.interval_us, outcome.pace.late, outcome.pace.frames,
    );
    for note in &outcome.notes {
        println!("NOTE\t{}\t{note}", outcome.scenario);
    }
    println!(
        "ESCALATION\t{}\t{}\t{}\t{}\t{}\t{}",
        outcome.scenario,
        outcome.counters.primitives_emitted,
        outcome.counters.primitives_culled,
        outcome.counters.bounds_tree_inserts,
        outcome.counters.chunks_reencoded,
        outcome.counters.chunks_translated,
    );
    for measurement in &outcome.measurements {
        println!("  {measurement}");
    }
}
