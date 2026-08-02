//! The counters ledger.
//!
//! Every counter in the frame's counter block is either incremented by some crate's shipped code,
//! or listed as awaiting the stage that will increment it. Nothing else is permitted, because a
//! counter with no producer reads zero forever and every budget written against it — "this
//! interaction repaints at most four fragments" — passes without measuring anything. Those are the
//! assertions hardest to notice are worthless: they are green, they name the right quantity, and
//! they are about a number that cannot move.
//!
//! The list of counters awaiting a producer is the other half. It is what separates a counter whose
//! stage has not been written yet from a counter whose producer was deleted, and it shrinks: a
//! counter that has acquired a producer while still being listed is a violation too, so the promise
//! is retired by the change that keeps it.

mod awaiting;
pub(crate) mod declared;
mod produced;
pub(crate) mod shipped;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let Some(declaration) = declared::find(tree) else {
        report.skip("no crate in this tree declares a counter set".to_owned());
        return report;
    };
    let producers = produced::find(tree, &declaration);

    for counter in &declaration.counters {
        match (producers.contains_key(counter), awaiting::stage_of(counter)) {
            (true, None) | (false, Some(_)) => {}
            (false, None) => report.violation(
                declaration.file.clone(),
                format!(
                    "`Counter::{counter}` is declared and nothing increments it, so it reads zero \
                     forever and any budget naming it passes without measuring anything. Wire the \
                     producer, delete the counter, or list it as awaiting its stage."
                ),
            ),
            (true, Some(stage)) => report.violation(
                "xtask/src/ledger/check/counters/awaiting.rs".to_owned(),
                format!(
                    "`Counter::{counter}` is listed as awaiting {stage}, but {} increments it now: \
                     drop the entry, and add the assertion that consumes the counter",
                    producers[counter].join(", ")
                ),
            ),
        }
    }
    report
}
