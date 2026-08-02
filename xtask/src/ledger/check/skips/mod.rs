//! The skips ledger.
//!
//! A counter of work *performed* is falsifiable on its own. A counter of work *avoided* is not: it
//! reads zero when the stage is skipping perfectly, zero when the stage has stopped skipping, and
//! zero when nobody ever incremented it. So every bound written against one alone — "this
//! interaction re-encodes no more than four ranges" — is satisfied by a stage that was deleted.
//!
//! The counter table says which counters are of the second kind, by declaring them
//! `Group::Skip { done: Counter::… }`. This check requires each of them to carry the two things
//! that make it mean something:
//!
//! * a **pair** — the named counter of work performed must exist, must be a different counter, and
//!   must not itself be a skip, because two counters with the same blind spot answer nothing;
//! * a **non-vacuity assertion** — a call to `assert_non_vacuous` in some member's test code,
//!   which drives the situation the skip exists for and requires the counter to move, then drives
//!   one in which reusing an answer would be wrong and requires it to stay where it was.
//!
//! Both are read off the tree, so a skip added without them fails the build rather than joining a
//! list somebody has to remember to keep.
//!
//! What a source tree can be asked is whether the assertion *exists*. Whether it is *taken* is a
//! question about a run, and the answer to it is [`super::ignored`]: an assertion this check found
//! and the runner never reached would leave this gate green over a proof nobody makes.

mod pair;
mod proof;

use crate::ledger::check::counters::declared;
use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// Where the assertion lives, for a message that says what to do next.
const ASSERTION: &str = "zgui_profile::counter::non_vacuity::assert_non_vacuous";

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let Some(declaration) = declared::find(tree) else {
        report.skip("no crate in this tree declares a counter set".to_owned());
        return report;
    };
    let Some(text) = table_text(tree, &declaration.file) else {
        report.skip(format!("{} could not be read again", declaration.file));
        return report;
    };

    let pairs = pair::pairs(text);
    let skipped: Vec<String> = pairs.iter().map(|pair| pair.skipped.clone()).collect();
    let proofs = proof::find(tree, &skipped);

    for entry in &pairs {
        check_pair(&mut report, &declaration, entry, &skipped);
        if !proofs.contains_key(&entry.skipped) {
            report.violation(
                declaration.file.clone(),
                format!(
                    "`Counter::{}` counts work that was avoided and no test proves it can move. It \
                     reads zero when the stage is perfect and zero when the stage is gone, so every \
                     bound written against it passes without measuring anything. Add a call to \
                     `{ASSERTION}` naming it, giving the situation the skip exists for and one in \
                     which skipping would be wrong.",
                    entry.skipped
                ),
            );
        }
    }
    report
}

/// Checks the pair half of one declaration.
fn check_pair(
    report: &mut Report,
    declaration: &declared::Declaration,
    entry: &pair::Pair,
    skipped: &[String],
) {
    let at = declaration.file.clone();
    let Some(done) = &entry.done else {
        report.violation(
            at,
            format!(
                "`Counter::{}` is declared a skip and names no counter of work performed. Write \
                 `Group::Skip {{ done: Counter::… }}`, naming what the stage does when it cannot \
                 reuse an answer.",
                entry.skipped
            ),
        );
        return;
    };
    if done == &entry.skipped {
        report.violation(
            at,
            format!(
                "`Counter::{}` is its own pair, so the two numbers a reader is asked to compare \
                 are one number. Name the counter of work performed.",
                entry.skipped
            ),
        );
    } else if skipped.contains(done) {
        report.violation(
            at,
            format!(
                "`Counter::{}` is read against `Counter::{done}`, which is itself a count of \
                 avoided work. Both can be zero for ever, so the comparison says nothing. Name a \
                 counter of work performed.",
                entry.skipped
            ),
        );
    } else if !declaration.counters.contains(done) {
        report.violation(
            at,
            format!(
                "`Counter::{}` is read against `Counter::{done}`, which this table does not \
                 declare.",
                entry.skipped
            ),
        );
    }
}

/// The counter table's text, found again in the tree by the path the declaration came from.
fn table_text<'a>(tree: &'a Tree, file: &str) -> Option<&'a str> {
    tree.members
        .iter()
        .flat_map(|member| &member.sources)
        .find(|source| source.rel_path == file)
        .map(|source| source.text.as_str())
}

#[cfg(test)]
mod tests;
