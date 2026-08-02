//! The standing gate over the reference workloads.
//!
//! Three documents, driven headless, each publishing a slope and stating what it is allowed to be
//! as a **same-run ratio** — the ratio of two quantities measured in one process over the same four
//! documents, which is dimensionless and therefore the same number on a laptop and on a loaded CI
//! host. The slopes themselves are printed in real units beside the verdicts, marked advisory, and
//! gate nothing: a criterion in microseconds can only be made to pass again by recording it again,
//! and a gate that is re-recorded whenever it fires gates nothing.
//!
//! What this adds over "run the binaries" is the names. A workload whose criterion was deleted,
//! renamed, or fed by a sweep that stopped running would exit zero and be green — so every
//! criterion each workload must still state is listed in [`subject`], checked against what the
//! workload actually printed, and only then is the run's own verdict believed.
//!
//! In release, for the same reason the wall-clock budgets are: a slope measured in an unoptimised
//! build is a slope of a program nobody runs.

mod subject;

use std::path::Path;

use crate::error::{Error, Result};
use crate::process;
use crate::workloads::subject::{HERE, WORKLOADS};

/// The harness that owns the workloads.
const HARNESS: &str = "zgui-bench";

/// Runs every reference workload and checks each still states its criteria.
pub(crate) fn run(root: &Path) -> Result<()> {
    let cargo = process::cargo();
    for workload in WORKLOADS {
        println!("workloads {:<14} {}", workload.binary, workload.about);
        let output = process::capture(
            root,
            &cargo,
            &["run", "--release", "-p", HARNESS, "--bin", workload.binary],
        )?;
        print!("{output}");
        let missing: Vec<&str> = workload
            .required
            .iter()
            .copied()
            .filter(|name| !states(&output, name))
            .collect();
        if !missing.is_empty() {
            return Err(Error::failed(gone(workload.binary, &missing, &output)));
        }
    }
    Ok(())
}

/// Whether `output` carries a verdict for the criterion called `name`.
///
/// A *verdict*, not a mention: the name has to be followed by `ok` or by `REGRESSION`, so a
/// criterion that survives only inside an advisory line or a failure message does not count as
/// stated.
///
/// Shared with the differential oracles, which read a run the same way and would otherwise each
/// decide for themselves what counts as a claim having been made.
pub(crate) fn states(output: &str, name: &str) -> bool {
    output.lines().any(|line| {
        line.starts_with(name)
            && (line.contains(" ok ") || line.contains("REGRESSION") || line.contains("BROKEN"))
    })
}

/// What to say when a workload stopped stating something it is here for.
fn gone(binary: &str, missing: &[&str], output: &str) -> String {
    // Read back the same way `states` reads: a criterion is what a verdict line begins with. A
    // looser rule here — anything that looks like a name — would answer "nothing at all" for a
    // workload that renamed every one of its criteria, which is the one case this message is for.
    let held: Vec<&str> = output
        .lines()
        .filter(|line| {
            line.contains(" ok ") || line.contains("REGRESSION") || line.contains("BROKEN")
        })
        .filter_map(|line| line.split_whitespace().next())
        .filter(|word| !word.starts_with("ok"))
        .collect();
    format!(
        "{missing:?} {} gone from the `{binary}` workload. Running the binary anyway would leave \
         the gate green over the claim it exists to make, which is the one thing this gate is for. \
         Either put the criterion back, or — if it was deliberately retired — say so in `{HERE}`.\n\
         What the workload states now: {}",
        if missing.len() == 1 { "is" } else { "are" },
        if held.is_empty() {
            "nothing at all".to_owned()
        } else {
            held.join(", ")
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{gone, states};

    #[test]
    fn a_criterion_with_a_verdict_beside_it_counts_as_stated() {
        assert!(states(
            "LIST-glide ok  0.9037  (recorded 0.9200 +/-12%)",
            "LIST-glide"
        ));
        assert!(states(
            "LIST-glide REGRESSION  1.4000, recorded 0.92",
            "LIST-glide"
        ));
        assert!(states(
            "LIST-glide BROKEN: the sizes did not determine a line",
            "LIST-glide"
        ));
    }

    #[test]
    fn a_criterion_that_is_only_mentioned_does_not() {
        // The failure this check exists for: a workload that still prints the name in an advisory
        // line or a header while no longer judging anything by it.
        assert!(!states(
            "  advisory  LIST-glide slope 28216.6 ns per box",
            "LIST-glide"
        ));
        assert!(!states("STATIC-locality ok  0.0702", "LIST-glide"));
    }

    #[test]
    fn a_workload_that_renamed_its_criteria_has_the_new_names_read_back_to_it() {
        // The message a person actually needs: not "nothing at all" but "here is what it says now".
        let message = gone(
            "list-slope",
            &["LIST-glide"],
            "LIST-scroll-cost ok  0.9146  (recorded 0.9200 +/-12%)\nok: 9 criteria, all inside\n",
        );
        assert!(message.contains("LIST-scroll-cost"), "{message}");
        assert!(!message.contains("nothing at all"), "{message}");
    }

    #[test]
    fn the_failure_names_what_is_missing_what_survives_and_where_to_edit() {
        let message = gone(
            "list-slope",
            &["LIST-glide"],
            "LIST-virtualisation-wheel ok  1.0024  (at most 1.0500)\nok: 8 criteria, all inside\n",
        );
        assert!(message.contains("LIST-glide"), "{message}");
        assert!(message.contains("list-slope"), "{message}");
        assert!(message.contains("LIST-virtualisation-wheel"), "{message}");
        assert!(
            message.contains("xtask/src/workloads/subject.rs"),
            "{message}"
        );
    }

    #[test]
    fn a_workload_that_printed_nothing_says_so_rather_than_listing_an_empty_set() {
        let message = gone("static-slope", &["STATIC-locality"], "");
        assert!(message.contains("nothing at all"), "{message}");
    }
}
