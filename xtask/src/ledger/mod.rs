//! The mechanical gates that hold the architecture in place.
//!
//! Each check is a pure function of a gathered [`Tree`], which is why each of them can be aimed
//! at a planted-violation fixture as easily as at the workspace itself. `--self-test` does
//! exactly that: a check nobody has ever seen fail is a check nobody should trust.

pub(crate) mod check;
pub(crate) mod phases;
pub(crate) mod report;
pub(crate) mod self_test;
pub(crate) mod tree;

use std::path::Path;

use crate::error::{Error, Result};
use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::features::FeatureSource;

/// Runs every check, or the one named by `only`, against the workspace.
pub(crate) fn run(root: &Path, only: Option<&str>) -> Result<()> {
    let tree = Tree::gather(root, FeatureSource::Cargo)?;
    let checks: Vec<&check::Check> = match only {
        None => check::CHECKS.iter().collect(),
        Some(name) => vec![check::find(name).ok_or_else(|| {
            Error::failed(format!(
                "no ledger check named `{name}`; there are {}",
                check::CHECKS
                    .iter()
                    .map(|check| check.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?],
    };

    let mut failed = 0;
    for check in checks {
        let report = (check.run)(&tree);
        print(check, &report);
        if !report.is_clean() {
            failed += 1;
        }
    }
    if failed == 0 {
        Ok(())
    } else {
        Err(Error::failed(format!("{failed} ledger check(s) failed")))
    }
}

/// Prints one report.
fn print(check: &check::Check, report: &Report) {
    if report.is_clean() {
        println!("ledger {:<12} ok    {}", check.name, check.description);
    } else {
        println!("ledger {:<12} FAIL  {}", check.name, check.description);
    }
    for violation in &report.violations {
        println!("    {violation}");
    }
    for skipped in &report.skipped {
        println!("    (skipped: {skipped})");
    }
}
