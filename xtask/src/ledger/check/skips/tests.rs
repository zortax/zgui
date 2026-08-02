//! What the skips gate accepts and what it refuses.
//!
//! Two of these run the check against the miniature workspaces under `xtask/fixtures/skips/`. The
//! rest hand it a tree assembled here, because the malformed declarations — a skip that is its own
//! pair, a skip read against another skip — are ones the real table cannot be left holding while
//! the suite is green.

use crate::ledger::check::skips::check;
use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::features::FeatureSource;

/// Runs the check against the fixture named by `expectation`.
fn fixture(expectation: &str) -> Report {
    let root = crate::root::workspace_root().expect("workspace root");
    let tree = Tree::gather(
        &root.join("xtask/fixtures/skips").join(expectation),
        FeatureSource::Recorded,
    )
    .expect("the fixture workspace is readable");
    check(&tree)
}

#[test]
fn skips_gate_fails_on_a_counter_without_a_test() {
    // The planted fixture is the shape this gate exists for and the shape nothing else catches: a
    // skip counter with a producer, a pair, and a test target beside it whose only assertion is an
    // upper bound — which is green over a stage that skips nothing and green over a stage that was
    // deleted.
    let report = fixture("planted");
    assert!(!report.is_clean(), "the planted fixture was accepted");
    let message = report.violations[0].to_string();
    assert!(message.contains("Counter::Beta"), "{message}");
    assert!(
        message.contains("assert_non_vacuous"),
        "a failure that does not say what to write next is half a gate: {message}"
    );
}

#[test]
fn the_gate_accepts_a_skip_that_carries_its_pair_and_its_proof() {
    let report = fixture("clean");
    assert!(
        report.is_clean(),
        "the clean fixture was rejected: {}",
        report.violations[0]
    );
}

#[test]
fn the_workspaces_own_skips_all_carry_both() {
    let root = crate::root::workspace_root().expect("workspace root");
    let tree = Tree::gather(&root, FeatureSource::Recorded).expect("the workspace is readable");
    let report = check(&tree);
    assert!(
        report.is_clean(),
        "{}",
        report
            .violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
