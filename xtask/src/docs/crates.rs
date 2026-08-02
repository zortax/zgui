//! Crate-level documentation, which is the only page a reader arriving at a crate is shown.

use std::path::Path;

use crate::error::{Result, read_to_string};
use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// How many lines of crate-level documentation are too few to say what a crate is for.
///
/// The number is low on purpose: this catches a crate with a one-line placeholder, not a crate
/// whose documentation is merely shorter than its neighbours'.
const MINIMUM_LINES: usize = 3;

/// Checks that every published crate says what it is for and refuses undocumented items.
pub(crate) fn check(root: &Path, tree: &Tree) -> Result<Report> {
    let mut report = Report::clean();
    let mut checked = 0;

    for member in &tree.members {
        if !member.manifest.is_published() {
            continue;
        }
        let lib = root.join(&member.rel_dir).join("src").join("lib.rs");
        if !lib.exists() {
            report.violation(
                member.manifest.rel_path.clone(),
                "published with no library target to document".to_owned(),
            );
            continue;
        }
        checked += 1;
        let text = read_to_string(&lib)?;
        let documentation = text
            .lines()
            .filter(|line| line.trim_start().starts_with("//!"))
            .count();
        if documentation < MINIMUM_LINES {
            report.violation(
                format!("{}/src/lib.rs", member.rel_dir),
                format!(
                    "{documentation} lines of crate documentation: say what the crate is for and \
                     which seam it sits on"
                ),
            );
        }
        if !text.contains("deny(missing_docs)") {
            report.violation(
                format!("{}/src/lib.rs", member.rel_dir),
                "no `#![deny(missing_docs)]`: an undocumented public item would ship".to_owned(),
            );
        }
    }

    if checked == 0 {
        report.skip("no published crates to read".to_owned());
    } else {
        println!("    read the crate documentation of {checked} published crates");
    }
    Ok(report)
}
