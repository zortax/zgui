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
//!
//! The check also holds the release build: no normal dependency entry of a member turns the counters
//! on. A crate feature that forwards to them is allowed, because an application can turn it off.

mod awaiting;
pub(crate) mod declared;
mod produced;
mod release;
pub(crate) mod shipped;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    for member in &tree.members {
        release::check(&member.name, &member.manifest, &mut report);
    }
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

#[cfg(test)]
mod tests {
    use super::check;
    use crate::ledger::report::Report;
    use crate::ledger::tree::Tree;
    use crate::ledger::tree::features::FeatureSource;

    /// Runs the check against the fixture named by `expectation`.
    fn fixture(expectation: &str) -> Report {
        let root = crate::root::workspace_root().expect("workspace root");
        let tree = Tree::gather(
            &root.join("xtask/fixtures/counters").join(expectation),
            FeatureSource::Recorded,
        )
        .expect("the fixture workspace is readable");
        check(&tree)
    }

    #[test]
    fn the_planted_fixture_fails_on_the_counter_only_tests_move() {
        // One violation exactly: a test-only increment that counts would clear `Beta`, and a cut
        // that hides the code below a test-only method would add `Alpha`.
        let report = fixture("planted");
        let messages: Vec<String> = report.violations.iter().map(ToString::to_string).collect();
        assert_eq!(messages.len(), 1, "{messages:#?}");
        assert!(messages[0].contains("`Counter::Beta`"), "{messages:#?}");
    }

    #[test]
    fn the_clean_fixture_counts_an_increment_below_a_test_only_method() {
        let report = fixture("clean");
        assert!(report.skipped.is_empty(), "{:#?}", report.skipped);
        assert!(
            report.is_clean(),
            "the clean fixture was rejected: {}",
            report.violations[0]
        );
    }
}
