//! Proving the ledgers can fail.
//!
//! Every check owns two miniature workspaces under `xtask/fixtures/<check>/`: `clean/`, which
//! it must accept, and `planted/`, which carries the exact violation the check exists to catch
//! and which it must reject. A gate that has never been observed failing is decoration.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::ledger::check::{self, Check};
use crate::ledger::tree::Tree;
use crate::ledger::tree::features::FeatureSource;

/// Runs the self-test, then the whole ledger against the workspace itself.
pub(crate) fn run(root: &Path) -> Result<()> {
    run_with(root, FeatureSource::Cargo)
}

/// The body of [`run`], with the feature-graph source chosen by the caller.
///
/// The in-process test harness passes [`FeatureSource::Recorded`], which finds no recording at
/// the workspace root, so that it never launches a second cargo while the first one is running.
pub(crate) fn run_with(root: &Path, features: FeatureSource) -> Result<()> {
    let mut failures = Vec::new();

    for check in check::CHECKS {
        for expectation in [Expectation::Clean, Expectation::Planted] {
            let directory = fixture_dir(root, check, expectation);
            if !directory.join("Cargo.toml").is_file() {
                failures.push(format!(
                    "ledger {:<12} has no {} fixture at {}",
                    check.name,
                    expectation.directory(),
                    directory.display()
                ));
                continue;
            }
            let tree = Tree::gather(&directory, FeatureSource::Recorded)?;
            let report = (check.run)(&tree);
            match (expectation, report.is_clean()) {
                (Expectation::Clean, true) => {
                    println!("ledger {:<12} ok    accepts its clean fixture", check.name);
                }
                (Expectation::Planted, false) => {
                    println!(
                        "ledger {:<12} ok    rejects its planted fixture, {} violation(s):",
                        check.name,
                        report.violations.len()
                    );
                    for violation in &report.violations {
                        println!("        {violation}");
                    }
                }
                (Expectation::Clean, false) => failures.push(format!(
                    "ledger {:<12} rejected its clean fixture: {}",
                    check.name, report.violations[0]
                )),
                (Expectation::Planted, true) => failures.push(format!(
                    "ledger {:<12} accepted its planted fixture at {}",
                    check.name,
                    directory.display()
                )),
            }
        }
    }

    let tree = Tree::gather(root, features)?;
    for check in check::CHECKS {
        let report = (check.run)(&tree);
        if report.is_clean() {
            println!("ledger {:<12} ok    accepts the workspace", check.name);
        } else {
            for violation in &report.violations {
                failures.push(format!("ledger {:<12} {violation}", check.name));
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::failed(format!(
            "the ledger self-test found {} problem(s):\n    {}",
            failures.len(),
            failures.join("\n    ")
        )))
    }
}

/// Which of a check's two fixtures is being run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expectation {
    /// A tree the check must accept.
    Clean,
    /// A tree carrying a planted violation, which the check must reject.
    Planted,
}

impl Expectation {
    /// The fixture subdirectory name.
    fn directory(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Planted => "planted",
        }
    }
}

/// Where a check's fixture lives.
fn fixture_dir(root: &Path, check: &Check, expectation: Expectation) -> PathBuf {
    root.join("xtask/fixtures")
        .join(check.name)
        .join(expectation.directory())
}

#[cfg(test)]
mod tests {
    use crate::ledger::tree::features::FeatureSource;

    #[test]
    fn every_ledger_check_fails_on_its_planted_fixture() {
        let root = crate::root::workspace_root().expect("workspace root");
        super::run_with(&root, FeatureSource::Recorded).expect("ledger self-test");
    }
}
