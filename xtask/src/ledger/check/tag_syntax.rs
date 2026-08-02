//! No view is written in the spelling the grammar replaced.
//!
//! A view is a call and a block. The spelling that came before it wrote tags, and the two are not
//! read by two front ends: the tag spelling is a parse error, so a view left behind in a `.rs`
//! file stops the build and needs no gate. What needs one is the half the compiler never reads —
//! a fence in a document, a snippet in a guide — which goes stale silently and is copied by the
//! next person to write a view.
//!
//! The check is lexical and scoped to a view's own text: a `</` or a `/>` inside the braces of a
//! `view!` is a tag, and neither can be written any other way. A sentence *about* the tag spelling
//! is not one, which is why this file may describe what it forbids.

mod scan;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// The two files that write a tag on purpose: the scanner that looks for one, and the fixture
/// asserting the message a tag now gets. Both would be pointless if they could not contain one.
const PERMITTED: [&str; 2] = [
    "xtask/src/ledger/check/tag_syntax/scan.rs",
    "crates/zgui-view-macro/tests/ui/tag_in_node_position.rs",
];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let sources = tree
        .members
        .iter()
        .flat_map(|member| member.sources.iter())
        .chain(tree.prose.iter())
        .filter(|file| !PERMITTED.contains(&file.rel_path.as_str()));
    for file in sources {
        for (line, found) in scan::tags(&file.text) {
            report.violation(
                format!("{}:{line}", file.rel_path),
                message(&file.rel_path, found),
            );
        }
    }
    report
}

/// What to say about one tag, which depends on whether a compiler would have said it first.
fn message(path: &str, found: &str) -> String {
    let unread = if path.ends_with(".md") {
        " Nothing compiles this, so nothing else will say so."
    } else {
        ""
    };
    format!(
        "`{found}` inside a `view!` is the spelling this grammar replaced: a node is written \
         `row(class = \"a\") {{ \"hi\" }}`, and it is closed by its own brace.{unread}"
    )
}

#[cfg(test)]
mod tests {
    use super::check;
    use crate::ledger::tree::Tree;
    use crate::ledger::tree::features::FeatureSource;

    /// Runs the check against the fixture named by `expectation`.
    fn fixture(expectation: &str) -> crate::ledger::report::Report {
        let root = crate::root::workspace_root().expect("workspace root");
        let tree = Tree::gather(
            &root.join("xtask/fixtures/tag-syntax").join(expectation),
            FeatureSource::Recorded,
        )
        .expect("the fixture workspace is readable");
        check(&tree)
    }

    #[test]
    fn the_gate_fails_on_a_document_whose_fence_still_writes_tags() {
        let report = fixture("planted");
        assert!(!report.is_clean(), "the planted fixture was accepted");
        let violation = report.violations[0].to_string();
        assert!(violation.contains("docs/guide.md:6"), "{violation}");
        assert!(violation.contains("closed by its own brace"), "{violation}");
        assert!(violation.contains("Nothing compiles this"), "{violation}");
    }

    #[test]
    fn the_gate_accepts_a_document_whose_fence_writes_a_call_and_a_block() {
        let report = fixture("clean");
        assert!(
            report.is_clean(),
            "the clean fixture was rejected: {}",
            report.violations[0]
        );
    }

    #[test]
    fn the_workspace_holds_no_view_written_the_way_that_has_gone() {
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
}
