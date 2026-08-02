//! Running the wall-clock budgets, and the one list that says where they are.
//!
//! A budget is an assertion about time, so it means nothing in an unoptimised build and nothing at
//! all if it is never executed. Both halves are handled here: every crate that owns budgets puts
//! them in a test target named `wall_clock`, behind a `wall-clock` feature so that the ordinary
//! debug run cannot execute them, and this runs each of those targets in release as part of the
//! definition of done.
//!
//! [`WALL_CLOCK_MEMBERS`] is the whole of the wiring, and it is not maintained by hand alone: the
//! `clock` ledger check holds it against the tree, so a crate that grows such a target without
//! being named here fails the gate, and a name here that points at no target fails it too.

use std::path::Path;

use crate::error::Result;
use crate::{lint, process};

/// The workspace members that own a `wall_clock` test target.
pub(crate) const WALL_CLOCK_MEMBERS: &[&str] = &["zgui-layout", "zgui"];

/// The test target every wall-clock budget lives in.
const TARGET: &str = "wall_clock";

/// The feature that target is behind.
const FEATURE: &str = "wall-clock";

/// Lints and then runs every budget target, in an optimised build.
pub(crate) fn run(root: &Path) -> Result<()> {
    let cargo = process::cargo();
    for member in WALL_CLOCK_MEMBERS {
        // The workspace clippy run cannot see a target behind a feature it does not turn on, so
        // the budgets would otherwise be the one code in the tree nothing lints.
        lint::feature_target(root, member, FEATURE, TARGET)?;
        process::run(
            root,
            &cargo,
            &[
                "test",
                "--release",
                "-p",
                member,
                "--features",
                FEATURE,
                "--test",
                TARGET,
            ],
            &[],
        )?;
    }
    Ok(())
}
