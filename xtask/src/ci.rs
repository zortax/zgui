//! The definition of done.
//!
//! A phase is complete when this is green on the pinned toolchain from a clean checkout. The
//! order is deliberate: the cheap gates run first so a formatting slip does not cost a full
//! workspace build to discover.

use std::path::Path;

use crate::error::Result;
use crate::{
    budget, cadence, docs, growth, ledger, lint, oracle, perf, process, release, resize, skips,
    tails, verify, wall_clock, workloads,
};

/// Runs every gate, stopping at the first failure.
pub(crate) fn run(root: &Path) -> Result<()> {
    let cargo = process::cargo();

    step("cargo fmt --check");
    process::run(root, &cargo, &["fmt", "--all", "--", "--check"], &[])?;

    step("cargo clippy (both profiles)");
    lint::run(root)?;

    step("cargo build");
    process::run(root, &cargo, &["build", "--workspace"], &[])?;

    step("cargo test");
    process::run(root, &cargo, &["test", "--workspace"], &[])?;

    // Named rather than left inside the step above, which already runs it. Several hundred views
    // live in rustdoc fences and nothing else compiles one, so the population deserves a line in
    // the log that says whether it built.
    step("cargo test --doc");
    process::run(root, &cargo, &["test", "--workspace", "--doc"], &[])?;

    // The three standing gates that need no optimised build. Each is a step of its own, and named,
    // because each is a claim every later phase is checked against rather than a property of this
    // commit — a claim folded into `cargo test` is one nobody can see fail on its own.
    //
    // The mechanical one first: it costs no build, and it is the one that says an assertion
    // somewhere else has stopped meaning anything.
    step("skips");
    skips::run(root)?;

    step("budget");
    budget::run(root)?;

    step("cadence");
    cadence::run(root)?;

    // In release, and always: a budget is an assertion about time, and one that only a
    // never-executed job could reach is an assertion about nothing. The debug run above cannot
    // execute these — the targets are behind a feature it does not turn on — so this is the only
    // place they run, and it runs every time.
    step("cargo test --release (wall-clock budgets)");
    wall_clock::run(root)?;

    // In release for the same reason, and beside the ratchet because it is the same kind of
    // instrument: a slope measured in an unoptimised build is a slope of a program nobody runs.
    step("resize");
    resize::run(root)?;

    // After `resize` and before the documentation gate, for the same reason `resize` sits where it
    // does: these are measurements in an optimised build, and they belong with the other ones
    // rather than among the checks that only read source.
    step("workloads");
    workloads::run(root)?;

    // The two differentials over what a frame is other than its pixels. In release and after the
    // measurements, because each is the whole script over two windows and one of them recomputes
    // everything it holds on every turn — and because what they are looking for is a stage that
    // stopped being maintained, which no amount of repeating makes more visible.
    step("hits");
    oracle::run(root, "hits")?;

    step("a11y-geom");
    oracle::run(root, "a11y-geom")?;

    // The oldest of the three, and beside them because it is the same experiment over the thing
    // they were built to extend: what the frame *draws*. It is last of the three because it is the
    // longest — seven documents rather than four — and because a disagreement about a hit or a
    // published rectangle is cheaper to read than one about six hundred primitives.
    step("verify");
    verify::run(root)?;

    // After the budgets and before the documentation gate, because this *writes* documentation:
    // `docs/performance.md` is regenerated from the run it just took, so a build whose numbers
    // moved leaves the change in the working tree where a reader will see it.
    step("performance ratchet");
    perf::run(root)?;

    // Immediately after the ratchet, and reading what it just stored rather than taking its own
    // measurement: the claim being checked is about the numbers a reader will see in
    // `docs/performance.md`, and a second opinion taken from a second run would be about a
    // different one.
    step("tails");
    tails::run(root)?;

    // A count band of zero over everything a run keeps, which is the one gate in this list that
    // can see a defect no timing can: a table that grows makes no frame slower until thousands of
    // frames later, and by then nothing attributes the cost to what caused it.
    step("growth");
    growth::run(root)?;

    step("cargo doc");
    process::run(
        root,
        &cargo,
        &["doc", "--workspace", "--no-deps"],
        &[("RUSTDOCFLAGS", "-D warnings")],
    )?;

    // After rustdoc has been built, because a documentation gate over sources rustdoc rejects is
    // a gate reporting on something nobody can read anyway.
    step("docs");
    docs::run(root)?;

    step("release");
    release::run(root)?;

    step("ledger");
    ledger::run(root, None)
}

/// Announces a gate, so a failure is easy to attribute in a scrolling log.
fn step(name: &str) {
    println!("\n=== {name} ===");
}
