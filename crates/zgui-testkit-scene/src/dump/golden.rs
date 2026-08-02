//! Comparing a rendering against the file that records it, and rewriting that file on request.

use std::fs;
use std::path::Path;

use crate::dump::diff;

/// The environment variable that turns a comparison into a rewrite.
pub const BLESS: &str = "ZGUI_BLESS";

/// Whether this run has been asked to rewrite goldens rather than check them.
pub fn is_blessing() -> bool {
    std::env::var_os(BLESS).is_some_and(|value| !value.is_empty() && value != "0")
}

/// The outcome of one comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The rendering matched the file.
    Matched,
    /// The file was missing.
    Missing,
    /// The rendering differed, with a report of the first difference.
    Differed(String),
}

/// Compares `actual` against the file at `path`, without rewriting anything.
///
/// # Errors
///
/// A missing file is [`Outcome::Missing`] rather than an error, because it is the ordinary state of
/// a golden that has not been written yet — and it is deliberately not a silent success.
pub fn compare(path: &Path, actual: &str) -> Outcome {
    let Ok(expected) = fs::read_to_string(path) else {
        return Outcome::Missing;
    };
    match diff::first_difference(&expected, actual) {
        None => Outcome::Matched,
        Some(report) => Outcome::Differed(report),
    }
}

/// Writes `actual` to `path`, creating the directories above it.
///
/// # Panics
///
/// Panics when the file cannot be written, because a bless that silently failed would leave the
/// author believing a golden had been updated.
pub fn write(path: &Path, actual: &str) {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .unwrap_or_else(|error| panic!("could not create {}: {error}", directory.display()));
    }
    fs::write(path, actual)
        .unwrap_or_else(|error| panic!("could not write {}: {error}", path.display()));
}

/// Asserts that `actual` is what the golden at `path` records.
///
/// # Blessing
///
/// With [`BLESS`] set, a golden that differs — or that does not exist — is **written and the test
/// still fails**, naming what changed. That is deliberate and is the difference between blessing
/// and disabling: a run that rewrites the record and reports success has checked nothing, and a
/// developer who blessed by reflex would have no moment at which to read the diff. The next run,
/// without the variable, is the one that passes.
///
/// # Panics
///
/// Panics when the rendering differs from the golden, and when the golden does not exist. A missing
/// golden is never quietly created: an assertion against a file that materialises on first run is
/// an assertion that has never once been checked.
pub fn assert_matches(path: &Path, actual: &str) {
    let outcome = compare(path, actual);
    let blessing = is_blessing();
    match (&outcome, blessing) {
        (Outcome::Matched, _) => {}
        (Outcome::Missing, false) => panic!(
            "no golden at {}. Re-run with {BLESS}=1 to write it, then read what it says before \
             committing it.",
            path.display()
        ),
        (Outcome::Missing, true) => {
            write(path, actual);
            panic!(
                "wrote a new golden at {}. Read it, then re-run without {BLESS} to check it.",
                path.display()
            );
        }
        (Outcome::Differed(report), false) => panic!(
            "{} does not match the rendering.\n{report}\nRe-run with {BLESS}=1 to accept the new \
             rendering.",
            path.display()
        ),
        (Outcome::Differed(report), true) => {
            write(path, actual);
            panic!(
                "rewrote {}.\n{report}\nRe-run without {BLESS} to check the new rendering.",
                path.display()
            );
        }
    }
}

/// Asserts that `tree` renders to the golden at `path`.
pub fn assert_tree(path: &Path, tree: &dyn crate::dump::TreeDump) {
    assert_matches(path, &crate::dump::to_text(tree));
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Outcome, compare, is_blessing, write};

    /// A path under the target directory, unique to this test binary and name.
    fn scratch(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push("zgui-testkit-scene-goldens");
        path.push(format!("{name}-{}.txt", std::process::id()));
        path
    }

    #[test]
    fn a_missing_golden_is_never_a_match() {
        let path = scratch("absent");
        let _ = std::fs::remove_file(&path);
        assert_eq!(compare(&path, "anything\n"), Outcome::Missing);
    }

    #[test]
    fn a_written_golden_matches_what_was_written_and_nothing_else() {
        let path = scratch("written");
        write(&path, "one\ntwo\n");
        assert_eq!(compare(&path, "one\ntwo\n"), Outcome::Matched);
        assert!(matches!(
            compare(&path, "one\nthree\n"),
            Outcome::Differed(_)
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn blessing_is_off_unless_the_variable_says_otherwise() {
        // The suite must never run in blessing mode: a blessed run rewrites the record it is
        // supposed to be checked against.
        assert!(!is_blessing(), "{} must not be set in CI", super::BLESS);
    }
}
