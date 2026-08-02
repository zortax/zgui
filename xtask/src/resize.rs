//! The standing gate over what one more box costs a resize.
//!
//! Resize is not what the compositor programme is about. Regressing it silently is not acceptable
//! either, and the shape it would regress into is specific: a configure that used to cost one
//! relayout and one full repaint starts costing something proportional to the document several
//! times over. That shows in the **slope** — microseconds per box across four document sizes —
//! long before it shows in any one number.
//!
//! Everything the comparison needs is inside the harness this runs, and deliberately so. The slope
//! is measured twice in one process over the same four documents: once for a configure, and once
//! for a change to the document's own content that forces the same relayout and full repaint by a
//! route that has nothing to do with the window's extent. What is compared against the recorded
//! value is the **ratio** of the two, which is dimensionless — a machine twice as fast halves both
//! slopes and leaves it where it was. The slope in microseconds per box is printed beside it and
//! gates nothing.
//!
//! In release, for the same reason the wall-clock budgets are: a slope measured in an unoptimised
//! build is a slope of a program nobody runs.

use std::path::Path;

use crate::error::Result;
use crate::process;

/// The harness that owns the measurement and the recorded ratio.
const HARNESS: &str = "zgui-bench";

/// The binary inside it.
const BINARY: &str = "resize-slope";

/// Builds the harness and takes the measurement.
pub(crate) fn run(root: &Path) -> Result<()> {
    let cargo = process::cargo();
    process::run(
        root,
        &cargo,
        &["run", "--release", "-p", HARNESS, "--bin", BINARY],
        &[],
    )
}
