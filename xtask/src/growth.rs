//! The growth gate: a count band of zero over everything a run keeps.
//!
//! The gate that would have caught the largest defect anyone has found in this renderer, on the day
//! it started firing. Nothing else can: a table that gains a hundred and seventy entries per wheel
//! notch makes no frame slower until thousands of notches later, so a timing band stays green
//! through the whole of the accumulation and a resident-set figure is smeared by the allocator and
//! lags by seconds. The length of the table is the number that discriminates, and a length is a
//! count, which means it reads the same on a slow machine, a fast one and under a debugger.
//!
//! One step, and everything it does is inside the harness that owns the counters. In release,
//! because a thousand ticks of an unoptimised gallery is minutes rather than seconds and the
//! quantity being checked does not depend on the optimisation level at all.

use std::path::Path;

use crate::error::Result;
use crate::process;

/// The harness that owns the live counts and the document they are read on.
const HARNESS: &str = "zgui-bench";

/// The reference workload the band is stated against.
const DOCUMENT: &str = "s13";

/// Drives the ticks and compares the counts.
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
            "growth",
            DOCUMENT,
        ],
        &[],
    )
}
