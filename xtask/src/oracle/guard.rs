//! The tests that cover what a two-window differential structurally cannot.
//!
//! # Why a differential cannot answer a question about correctness
//!
//! Both windows resolve geometry through the same code. An error in that code is therefore present
//! on both sides of the comparison, produces the same wrong answer twice, and cancels: the
//! differential sees nothing. Two mutations demonstrate it rather than argue it — resolving
//! accessibility bounds against no placements at all, and testing a clip in the fragment's own
//! space instead of the space the clip was measured in. Both are wrong everywhere, both leave
//! [`hits`](crate::oracle) and `a11y-geom` green, and both are caught within a second by a named
//! test over a known answer.
//!
//! What a differential *can* answer is whether the live window's incremental state has drifted from
//! what a rebuild produces, which no unit test over a fixture reaches. So the two kinds of check are
//! not substitutes, and the failure mode this module exists for is a reader who believes one of them
//! is doing the other's work.
//!
//! # Why the names are checked and not merely written down
//!
//! A sentence in a gate's documentation naming the test that covers correctness is true on the day
//! it is written. Renaming or deleting that test leaves the sentence behind, pointing at nothing,
//! and the gate goes on passing — which is the same failure this whole module is about, one level
//! up. So the names are data the gate reads, and a gate whose guard has left the tree fails before
//! it runs.

use std::path::Path;

use crate::error::{Error, Result};

/// One named test that covers correctness the differential above it cannot see.
pub(crate) struct Guard {
    /// Where the test is, relative to the workspace root.
    pub(crate) file: &'static str,
    /// What it is called.
    pub(crate) test: &'static str,
    /// The mutation it was watched failing, so that anyone can run the demonstration again.
    pub(crate) mutation: &'static str,
}

impl Guard {
    /// Whether the test this names is still in the file this names.
    fn present(&self, root: &Path) -> bool {
        std::fs::read_to_string(root.join(self.file))
            .is_ok_and(|source| source.contains(&format!("fn {}(", self.test)))
    }
}

/// Fails when a gate names a guard that is no longer in the tree.
///
/// # Errors
///
/// Fails when any of `guards` names a file that cannot be read or a test that is not in it.
pub(crate) fn check(root: &Path, gate: &str, guards: &[Guard]) -> Result<()> {
    let missing: Vec<&Guard> = guards.iter().filter(|guard| !guard.present(root)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(Error::failed(gone(gate, &missing)))
}

/// What to say when a gate's guard has left the tree.
fn gone(gate: &str, missing: &[&Guard]) -> String {
    let named: Vec<String> = missing
        .iter()
        .map(|guard| format!("`{}` in {} ({})", guard.test, guard.file, guard.mutation))
        .collect();
    format!(
        "the `{gate}` gate names {} as covering the correctness it cannot see itself, and {} not \
         in the tree. Both windows a differential compares resolve geometry through the same code, \
         so an error in that code cancels and this gate stays green over it — these tests are what \
         does not. Either put them back, or name what replaced them in {}.",
        named.join(", "),
        if missing.len() == 1 {
            "it is"
        } else {
            "they are"
        },
        super::subject::HERE,
    )
}

#[cfg(test)]
mod tests {
    use super::{Guard, check, gone};
    use crate::oracle::subject::ORACLES;
    use crate::root::workspace_root;

    #[test]
    fn every_gate_names_a_guard_and_every_guard_is_in_the_tree() {
        // The check itself, run without running either differential: a rename that empties a gate's
        // cover is caught by `cargo test` rather than only by the gate, which takes minutes.
        let root = workspace_root().expect("the workspace root");
        for oracle in ORACLES {
            assert!(
                !oracle.guarded_by.is_empty(),
                "{} claims nothing covers what it cannot see",
                oracle.gate,
            );
            check(&root, oracle.gate, oracle.guarded_by).expect("every named guard is in the tree");
        }
    }

    #[test]
    fn a_guard_that_has_left_the_tree_fails_the_gate_that_names_it() {
        let root = workspace_root().expect("the workspace root");
        let renamed = [Guard {
            file: "crates/zgui-runtime/tests/a11y.rs",
            test: "a_test_by_a_name_nothing_in_the_tree_has",
            mutation: "resolve bounds against no placements",
        }];
        let error = check(&root, "a11y-geom", &renamed).expect_err("the name is not in the file");
        assert!(
            error
                .to_string()
                .contains("a_test_by_a_name_nothing_in_the_tree_has"),
            "{error}",
        );
    }

    #[test]
    fn a_file_that_is_gone_altogether_is_the_same_failure_as_a_rename() {
        let root = workspace_root().expect("the workspace root");
        let moved = [Guard {
            file: "crates/zgui-runtime/tests/no-such-file.rs",
            test: "a_control_under_a_transform_is_reported_where_the_transform_puts_it",
            mutation: "resolve bounds against no placements",
        }];
        check(&root, "a11y-geom", &moved).expect_err("the file cannot be read");
    }

    #[test]
    fn the_failure_says_why_the_gate_cannot_cover_this_itself() {
        // Without this sentence the failure reads as a stale reference to tidy up, and the tidiest
        // way to satisfy it is to delete the line — which is exactly the state the gate was in
        // before any of this: green, and covering less than it claimed.
        let message = gone(
            "hits",
            &[&Guard {
                file: "crates/zgui-layout/tests/fragments/hits.rs",
                test: "a_transformed_box_answers_only_where_its_ancestors_clip_shows_it",
                mutation: "test the clip in the fragment's own space",
            }],
        );
        assert!(
            message.contains("resolve geometry through the same code"),
            "{message}"
        );
        assert!(message.contains("xtask/src/oracle/subject.rs"), "{message}");
    }
}
