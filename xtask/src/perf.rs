//! The performance ratchet.
//!
//! One step, and everything it does is inside the harness it runs: `zgui-bench` drives the five
//! end-to-end scenarios, compares every number it takes against the band recorded for it, stores
//! the run under `docs/perf/runs/`, regenerates `docs/performance.md` and exits non-zero naming
//! whatever left its band. Nothing is duplicated here, because a gate that re-implemented the
//! comparison would be a second opinion about the same numbers and the two would drift.
//!
//! In release, always, for the same reason the wall-clock budgets are: a band is an assertion about
//! time, and one measured in an unoptimised build is an assertion about a program nobody runs.

use std::path::Path;

use crate::error::Result;
use crate::process;

/// The harness that owns the scenarios and their bands.
const HARNESS: &str = "zgui-bench";

/// Builds the harness and runs the sweep.
pub(crate) fn run(root: &Path) -> Result<()> {
    let cargo = process::cargo();
    process::run(
        root,
        &cargo,
        &[
            "run",
            "--release",
            "-p",
            HARNESS,
            "--bin",
            HARNESS,
            "--",
            "scenarios",
        ],
        &[],
    )
}

/// Measures a scripted scroll's pacing over `size` for `seconds`.
///
/// **Not a step of `ci`, and it must not become one.** What it measures is the interval between
/// frames against the refresh of the output they were presented to, and an output is not a property
/// of a checkout: a gate that ran this under whatever the build machine has would be banding a
/// number about that machine's compositor. It is run by hand on the reference machine at phase
/// exit, and its result is published dated under `docs/perf/`.
///
/// What protects the same quantity between those runs is `growth`, which is counts, and `tails`,
/// which is shapes. Both are portable, and a leak is what turns a good pacing number into a bad one.
pub(crate) fn pacing(root: &Path, size: &str, seconds: f64) -> Result<()> {
    let cargo = process::cargo();
    process::run(
        root,
        &cargo,
        &[
            "run",
            "--release",
            "-p",
            HARNESS,
            "--bin",
            HARNESS,
            "--",
            "pacing",
            size,
            &format!("{seconds}"),
        ],
        &[],
    )
}
