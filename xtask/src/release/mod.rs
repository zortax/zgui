//! What has to be true for the tree to be released.
//!
//! Two questions, and they are separate. *Can this be released at all* — one version everywhere,
//! and every internal dependency expressed so that a registry can resolve it. And *may this be
//! released under that version* — whether the public surface still keeps the promise the last
//! release made.

pub(crate) mod lockstep;
pub(crate) mod semver;

use std::path::Path;

use crate::error::{Error, Result};
use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::features::FeatureSource;

/// Runs the lockstep check and then the compatibility gate.
pub(crate) fn run(root: &Path) -> Result<()> {
    let tree = Tree::gather(root, FeatureSource::Cargo)?;
    let report = lockstep::check(&tree);
    print(&report);
    if !report.is_clean() {
        return Err(Error::failed(format!(
            "{} lockstep violation(s); the tree cannot be published as it stands",
            report.violations.len()
        )));
    }
    semver::run(root)
}

/// Prints the lockstep report.
fn print(report: &Report) {
    if report.is_clean() {
        println!("lockstep   ok    one version, and every internal dependency states it");
    } else {
        println!("lockstep   FAIL");
    }
    for violation in &report.violations {
        println!("    {violation}");
    }
    for skipped in &report.skipped {
        println!("    (skipped: {skipped})");
    }
}
