//! Finding the non-vacuity assertion a skip counter is required to carry.

use std::collections::BTreeMap;

use crate::ledger::tree::Tree;
use crate::ledger::tree::sources::SourceFile;

/// The call a skip counter's proof is written as.
const CALL: &str = "assert_non_vacuous(Counter::";

/// The path fragment that marks a member's shipped code, whose test module is the tail of a file.
const SHIPPED: &str = "/src/";

/// The attribute that begins a module compiled only for tests.
const TEST_MODULE: &str = "#[cfg(test)]";

/// Where each skip counter's proof was found, by counter name.
pub(crate) type Proofs = BTreeMap<String, Vec<String>>;

/// Every counter in `skips` that some member's test code proves non-vacuous.
pub(crate) fn find(tree: &Tree, skips: &[String]) -> Proofs {
    let mut proofs = Proofs::new();
    for member in &tree.members {
        for file in &member.sources {
            let Some(text) = test_code_of(file) else {
                continue;
            };
            let code = code_of(text);
            for counter in skips {
                if code.contains(&format!("{CALL}{counter},")) {
                    proofs
                        .entry(counter.clone())
                        .or_default()
                        .push(file.rel_path.clone());
                }
            }
        }
    }
    proofs
}

/// The part of `file` that is compiled only for tests, or `None` when it has none.
///
/// A test target is test code whole. A shipped file's test code is whatever follows its
/// `#[cfg(test)]`, which is the same cut the counters ledger makes in the other direction: the
/// proof of a skip is an assertion, and an assertion that ships is not one.
fn test_code_of(file: &SourceFile) -> Option<&str> {
    if !file.rel_path.contains(SHIPPED) {
        return Some(&file.text);
    }
    file.text
        .find(TEST_MODULE)
        .map(|at| &file.text[at + TEST_MODULE.len()..])
}

/// `text` with its comment lines dropped and its whitespace removed.
///
/// The comments go first because a doc example is prose: the call written inside one demonstrates
/// the API and asserts nothing. The whitespace goes because a call whose arguments were wrapped
/// onto their own lines is the same call.
fn code_of(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(|line| line.chars().filter(|character| !character.is_whitespace()))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::ledger::tree::sources::SourceFile;

    use super::{code_of, test_code_of};

    /// A source file at `path` holding `text`.
    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            rel_path: path.to_owned(),
            text: text.to_owned(),
        }
    }

    /// Whether `text`, cut and stripped as the check cuts and strips it, proves `counter`.
    fn proves(path: &str, text: &str, counter: &str) -> bool {
        test_code_of(&file(path, text)).is_some_and(|text| {
            code_of(text).contains(&format!("assert_non_vacuous(Counter::{counter},"))
        })
    }

    #[test]
    fn a_call_in_a_test_target_counts_however_it_was_wrapped() {
        assert!(proves(
            "crates/a/tests/skips.rs",
            "assert_non_vacuous(\n    Counter::LayoutsHeld,\n    fires,\n    silent,\n);\n",
            "LayoutsHeld"
        ));
    }

    #[test]
    fn a_call_in_a_unit_test_module_counts_and_one_above_it_does_not() {
        let text = "fn ship() { assert_non_vacuous(Counter::Shipped, a, b); }\n\
                    #[cfg(test)]\nmod tests {\n assert_non_vacuous(Counter::Tested, a, b);\n}\n";
        assert!(proves("crates/a/src/lib.rs", text, "Tested"));
        assert!(
            !proves("crates/a/src/lib.rs", text, "Shipped"),
            "a call that ships is a call in the framework, not an assertion about it"
        );
    }

    #[test]
    fn a_call_written_inside_a_doc_example_proves_nothing() {
        // The exact way this gate would come to be satisfied by documentation: the rustdoc of the
        // assertion itself shows how to call it, and that rustdoc names a real counter.
        assert!(!proves(
            "crates/a/tests/skips.rs",
            "/// assert_non_vacuous(Counter::LayoutsHeld, fires, silent);\n",
            "LayoutsHeld"
        ));
    }

    #[test]
    fn a_longer_counter_name_that_starts_with_a_shorter_one_is_not_confused_for_it() {
        let text = "assert_non_vacuous(Counter::LayoutsHeldForEver, a, b);\n";
        assert!(proves("crates/a/tests/s.rs", text, "LayoutsHeldForEver"));
        assert!(!proves("crates/a/tests/s.rs", text, "LayoutsHeld"));
    }
}
