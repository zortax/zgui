//! The two standing gates over what a running window remembers, other than its pixels.
//!
//! A window is compared against a second window on the same document at the same size, driven
//! through the same script event for event, which throws every held layout result away before each
//! turn of its loop. That comparison already covers what the frame *draws*. These two extend it to
//! two things a display list does not contain:
//!
//! * `hits` — the elements under a point, over a grid, after every step;
//! * `a11y-geom` — the rectangles handed to a screen reader and to an input method.
//!
//! # What a green run means, exactly
//!
//! **The live window's incremental state answers what a rebuild answers.** Nothing more. Both
//! windows resolve geometry through the same code, so an error in that code is made twice, is the
//! same error both times, and cancels — this comparison is silent about it. Two mutations
//! demonstrate that rather than assert it: resolving accessibility bounds against no placements at
//! all, and testing a clip in the fragment's own space rather than the space it was measured in,
//! are both wrong at every point of every document and leave both gates green.
//!
//! What these gates do catch is *drift*, and nothing else in the project reaches it: a window that
//! has been running holds an index, a set of published rectangles and a chain of coordinate systems
//! that were brought up to date a piece at a time over ninety-five steps, and every one of those
//! updates is a chance to leave a piece behind. Dropping the deferred hierarchy update in
//! `HitIndex::carry` faults 49 of 85 steps here and no unit test at all.
//!
//! Correctness — whether a rebuild's own answer is the right one — is decided by the tests each
//! gate names in [`subject`], which are checked to still exist before the gate runs. See
//! [`guard`] for why naming them is part of the gate rather than a note beside it.
//!
//! In release, and over four documents each, because both are the whole script over two windows and
//! one of those windows recomputes everything it holds on every turn.
//!
//! # What a run has to say, and why the names are listed
//!
//! Running a phase and believing its exit code would be green when the criterion it exists for was
//! renamed, deleted, or fed by a comparison that stopped happening. So every criterion each size
//! must still state is in [`subject`], checked against what the run actually printed, and only then
//! is the run's own verdict believed.

mod guard;
mod skipped;
mod subject;

use std::path::Path;

use crate::error::{Error, Result};
use crate::oracle::subject::{HERE, ORACLES, Oracle};
use crate::process;
use crate::workloads::states;

/// The harness that owns the differentials.
const HARNESS: &str = "zgui-bench";

/// The binary inside it, named because the harness also ships the reference workloads and a
/// package with several binaries has no default one.
const BINARY: &str = "zgui-bench";

/// Runs the gate called `gate`.
///
/// # Errors
///
/// Fails when a run disagreed, when a run no longer states a criterion it is here for, and when the
/// tests that cover what this gate cannot see are no longer in the tree.
pub(crate) fn run(root: &Path, gate: &str) -> Result<()> {
    let oracle = ORACLES
        .iter()
        .find(|oracle| oracle.gate == gate)
        .ok_or_else(|| {
            Error::failed(format!("no oracle called `{gate}` is registered in {HERE}"))
        })?;
    // Before the run and not after it: a gate that has quietly become the only thing anyone looks
    // at is worse the longer it goes on being believed, and minutes of differential do not change
    // the answer.
    guard::check(root, gate, oracle.guarded_by)?;
    let cargo = process::cargo();
    println!("{gate} {:<10} {}", oracle.phase, oracle.about);
    println!(
        "  green means the live window agrees with a rebuild, not that either is right; \
         correctness is {}",
        oracle
            .guarded_by
            .iter()
            .map(|guard| guard.test)
            .collect::<Vec<_>>()
            .join(", "),
    );
    for size in oracle.sizes {
        let output = process::capture(
            root,
            &cargo,
            &[
                "run",
                "--release",
                "-p",
                HARNESS,
                "--bin",
                BINARY,
                "--",
                oracle.phase,
                size.name,
            ],
        )?;
        print!("{output}");
        let missing: Vec<&str> = size
            .required
            .iter()
            .copied()
            .filter(|name| !states(&output, name))
            .collect();
        if !missing.is_empty() {
            return Err(Error::failed(gone(oracle, size.name, &missing, &output)));
        }
        // And how much of the script it actually compared. A criterion that is still stated over a
        // run that declined half the steps is a claim about half the script wearing the name of one
        // about all of it.
        skipped::check(oracle.gate, size.name, size.skipped, &output, HERE)?;
    }
    Ok(())
}

/// What to say when a run stopped stating something it is here for.
fn gone(oracle: &Oracle, size: &str, missing: &[&str], output: &str) -> String {
    let held: Vec<&str> = output
        .lines()
        .filter(|line| {
            line.contains(" ok ") || line.contains("REGRESSION") || line.contains("BROKEN")
        })
        .filter_map(|line| line.split_whitespace().next())
        .filter(|word| !word.starts_with("ok"))
        .collect();
    format!(
        "{missing:?} {} gone from `{}` at {size}, which the {} gate runs because {}. A run that no \
         longer makes the claim it exists for must fail rather than pass quietly. Either put the \
         criterion back, or — if the document at that size genuinely cannot state it any more — \
         say so in `{HERE}`.\nWhat the run states now: {}",
        if missing.len() == 1 { "is" } else { "are" },
        oracle.phase,
        oracle.gate,
        oracle.about,
        if held.is_empty() {
            "nothing at all".to_owned()
        } else {
            held.join(", ")
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{ORACLES, gone};

    #[test]
    fn the_failure_names_what_is_missing_the_size_the_claim_and_where_to_edit() {
        let oracle = &ORACLES[1];
        let message = gone(
            oracle,
            "s13",
            &["caret_geometry_agrees_with_a_cold_window"],
            "a11y_and_caret_geometry_agree_with_a_cold_window ok  size=s13 compared=85\n",
        );
        assert!(
            message.contains("caret_geometry_agrees_with_a_cold_window"),
            "{message}"
        );
        assert!(message.contains("s13"), "{message}");
        assert!(message.contains("input method"), "{message}");
        assert!(message.contains("xtask/src/oracle/subject.rs"), "{message}");
        assert!(
            message.contains("a11y_and_caret_geometry_agree_with_a_cold_window"),
            "the run's surviving criteria are read back to it: {message}"
        );
    }

    #[test]
    fn a_run_that_printed_nothing_says_so_rather_than_listing_an_empty_set() {
        let message = gone(
            &ORACLES[0],
            "s0",
            &["hit_results_agree_with_a_cold_window"],
            "",
        );
        assert!(message.contains("nothing at all"), "{message}");
    }
}
