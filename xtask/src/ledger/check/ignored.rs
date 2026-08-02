//! No test in the tree is switched off.
//!
//! Every other gate in this workspace rests on the suite running. A test carrying the attribute
//! that disables it still compiles, still reads as covering its subject, and still appears in the
//! file a reviewer opens — it simply never executes, and the line the runner prints about it scrolls
//! past among thousands that say `ok`. So the assertion it holds becomes an assertion nobody makes,
//! and every gate that was satisfied by its existence goes on being satisfied.
//!
//! That is not hypothetical here. The gate in [`super::skips`] requires a counter of avoided work to
//! carry a test proving the counter can move; it establishes that by reading the source, because a
//! test's *existence* is what a source tree can be asked about. Disabling that test leaves the call
//! in the file, so the skips gate stays green while the proof it names has stopped being taken. One
//! attribute defeats it. This check is why it cannot.
//!
//! # There is no allowlist, and that is the point
//!
//! A test that cannot run everywhere — one needing a graphics device, say — refuses at run time
//! instead: it looks for what it needs, says on standard error that it did not find it, and returns.
//! The refusal is then a fact about the machine, printed where it happened, rather than a permanent
//! property of the source. The property such a test covers is also asserted by something that runs
//! everywhere, so the machine without the device still gates the claim, one level further out.
//!
//! An exemption list here would be the same hole with a form to fill in. If a case ever genuinely
//! needs one, it belongs in this file with its reason written out, the way [`super::inert`] lists
//! the variants it cannot see the construction of — and adding it should feel like the argument it
//! is.

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::sources::SourceFile;

/// The attribute that disables a test outright.
const DIRECT: &str = "#[ignore";

/// The attribute that disables one under a condition.
const CONDITIONAL: &str = "#[cfg_attr(";

/// What a conditional attribute has to apply for this check to be about it.
const APPLIED: &str = "ignore";

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    for member in &tree.members {
        for file in &member.sources {
            for (line, text) in disabled(file) {
                report.violation(
                    format!("{}:{line}", file.rel_path),
                    format!(
                        "`{text}` stops this test running, and a test that does not run asserts \
                         nothing. Every gate that was satisfied by its existence stays green \
                         without it. Fix what it caught, or delete it and say what stopped being \
                         checked; a test that needs something this machine may not have refuses at \
                         run time and prints why."
                    ),
                );
            }
        }
    }
    report
}

/// Every line of `file` that disables a test, as its number and what it says.
fn disabled(file: &SourceFile) -> impl Iterator<Item = (usize, &str)> {
    file.text
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| is_attribute(line))
}

/// Whether `line` is an attribute that disables a test.
///
/// An attribute rather than a mention of one: the line has to *begin* with it. A documentation
/// comment showing the attribute begins with a slash, a string naming it begins with a quote, and
/// this file's own constants begin with `const` — none of which is a test being switched off. The
/// formatting gate is what makes the beginning of a line the right place to look, because an
/// attribute that shares a line with the item it applies to is a formatting failure first.
///
/// A conditional attribute is read with its strings taken out, so the feature named
/// `ignore-slow-machines` in the condition is not mistaken for the attribute being applied.
fn is_attribute(line: &str) -> bool {
    if line.starts_with(DIRECT) {
        return true;
    }
    line.starts_with(CONDITIONAL)
        && unquoted(line)
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .any(|word| word == APPLIED)
}

/// `line` with everything between double quotes removed.
///
/// Splitting on the quote character alternates outside, inside, outside; the even pieces are the
/// ones that were not in a string.
fn unquoted(line: &str) -> String {
    line.split('"').step_by(2).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{check, is_attribute};
    use crate::ledger::report::Report;
    use crate::ledger::tree::Tree;
    use crate::ledger::tree::features::FeatureSource;

    /// Runs the check against the fixture named by `expectation`.
    fn fixture(expectation: &str) -> Report {
        let root = crate::root::workspace_root().expect("workspace root");
        let tree = Tree::gather(
            &root.join("xtask/fixtures/ignored").join(expectation),
            FeatureSource::Recorded,
        )
        .expect("the fixture workspace is readable");
        check(&tree)
    }

    /// The shape this gate exists for: the proof another gate reads is still in the file, and no
    /// longer runs.
    #[test]
    fn the_gate_fails_on_a_test_that_was_switched_off() {
        let report = fixture("planted");
        assert!(!report.is_clean(), "the planted fixture was accepted");
        let violation = report.violations[0].to_string();
        assert!(violation.contains("tests/skips.rs:6"), "{violation}");
        assert!(
            violation.contains("asserts nothing"),
            "a failure that does not say why is half a gate: {violation}"
        );
    }

    #[test]
    fn the_gate_accepts_a_suite_that_runs() {
        let report = fixture("clean");
        assert!(
            report.is_clean(),
            "the clean fixture was rejected: {}",
            report.violations[0]
        );
    }

    #[test]
    fn every_test_in_this_workspace_runs() {
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

    #[test]
    fn both_spellings_of_the_attribute_are_the_attribute() {
        assert!(is_attribute("#[ignore]"));
        assert!(is_attribute("#[ignore = \"needs a device\"]"));
        assert!(is_attribute("#[cfg_attr(miri, ignore)]"));
        assert!(is_attribute("#[cfg_attr(not(feature = \"slow\"), ignore)]"));
    }

    #[test]
    fn writing_about_the_attribute_is_not_using_it() {
        // Every way this file, its own tests and the documentation of the gate it defends name the
        // attribute without anything being switched off.
        for innocent in [
            "/// #[ignore]",
            "//! A test carrying #[ignore] never runs.",
            "// #[cfg_attr(miri, ignore)]",
            "const DIRECT: &str = \"#[ignore\";",
            "assert!(is_attribute(\"#[ignore]\"));",
            "#[test]",
            "#[cfg_attr(windows, should_panic)]",
            "#[should_panic(expected = \"ignore\")]",
            "#[cfg_attr(feature = \"ignore-slow-machines\", test)]",
        ] {
            assert!(!is_attribute(innocent), "fired on {innocent:?}");
        }
    }
}
