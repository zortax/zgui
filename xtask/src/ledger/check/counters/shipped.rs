//! Which text in a member counts as the code it ships.
//!
//! Two things are excluded, for one reason. A counter is *produced* by a stage doing work, so a
//! counter incremented only from a test — an integration test under `tests/`, a unit test module
//! behind `#[cfg(test)]` — is not produced at all: the budget written against it would be measuring
//! the test that moves it. The same cut applies to reading the counter set out of the tree, because
//! a set declared inside a test module is a fixture rather than the framework's counters.

use crate::ledger::tree::sources::SourceFile;

/// The path fragment that separates a member's shipped code from its test targets.
const SHIPPED: &str = "/src/";

/// The attribute that begins a module compiled only for tests.
const TEST_MODULE: &str = "#[cfg(test)]";

/// The part of `file` that is compiled into the crate, or `None` when the file is a test target.
pub(crate) fn code_of(file: &SourceFile) -> Option<&str> {
    if !file.rel_path.contains(SHIPPED) {
        return None;
    }
    Some(match file.text.find(TEST_MODULE) {
        Some(at) => &file.text[..at],
        None => &file.text,
    })
}

#[cfg(test)]
mod tests {
    use crate::ledger::tree::sources::SourceFile;

    use super::code_of;

    /// A source file at `path` holding `text`.
    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            rel_path: path.to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_test_target_ships_nothing() {
        assert_eq!(
            code_of(&file("crates/a/tests/budgets.rs", "fn a() {}")),
            None
        );
        assert_eq!(code_of(&file("crates/a/benches/b.rs", "fn a() {}")), None);
    }

    #[test]
    fn a_unit_test_module_is_cut_off_the_end_of_a_shipped_file() {
        let source = file(
            "crates/a/src/lib.rs",
            "fn ship() {}\n\n#[cfg(test)]\nmod tests {\n    fn pretend() {}\n}\n",
        );
        let code = code_of(&source).expect("a shipped file");
        assert!(code.contains("ship"));
        assert!(
            !code.contains("pretend"),
            "a counter moved only by a unit test is moved by the test and not by the framework"
        );
    }

    #[test]
    fn a_shipped_file_with_no_test_module_is_kept_whole() {
        let source = file("crates/a/src/lib.rs", "fn ship() {}\n");
        assert_eq!(code_of(&source), Some("fn ship() {}\n"));
    }
}
