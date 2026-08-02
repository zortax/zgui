//! The documentation gate.
//!
//! Three questions, each of which a reader would otherwise have to answer by being disappointed:
//! does every published crate say what it is for, do the guides exist and do their cross-references
//! resolve, and does any of it defer to notes a reader outside this repository cannot open.
//!
//! Rustdoc's own gates — undocumented public items, broken intra-doc links, code fences that do not
//! compile — are enforced by the compiler and by the test runner, not here. This gate is for the
//! part a compiler has no opinion about.

pub(crate) mod crates;
pub(crate) mod forbidden;
pub(crate) mod guides;
pub(crate) mod rustdoc;
pub(crate) mod scan;
pub(crate) mod sources;

use std::path::Path;

use crate::error::{Error, Result};
use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::features::FeatureSource;

/// Runs the documentation gate.
pub(crate) fn run(root: &Path) -> Result<()> {
    let tree = Tree::gather(root, FeatureSource::Cargo)?;
    let checks: [(&str, Report); 3] = [
        ("crates", crates::check(root, &tree)?),
        ("guides", guides::check(root)?),
        ("phrasing", rustdoc::check(root)?),
    ];

    let mut failed = 0;
    for (name, report) in &checks {
        print(name, report);
        if !report.is_clean() {
            failed += 1;
        }
    }
    if failed == 0 {
        Ok(())
    } else {
        Err(Error::failed(format!(
            "{failed} documentation check(s) failed"
        )))
    }
}

/// Prints one report.
fn print(name: &str, report: &Report) {
    if report.is_clean() {
        println!("docs {name:<10} ok");
    } else {
        println!("docs {name:<10} FAIL");
    }
    for violation in &report.violations {
        println!("    {violation}");
    }
    for skipped in &report.skipped {
        println!("    (skipped: {skipped})");
    }
}
