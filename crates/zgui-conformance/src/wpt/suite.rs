//! Finding the tests, running each against its reference, and reporting a rate per suite.

use std::path::{Path, PathBuf};

use crate::fragment;
use crate::wpt::markup;
use crate::zdoc::build::lay_out;
use crate::zdoc::source::Zdoc;

/// The environment variable that points the converter at a checked-out reference suite.
///
/// Without it the vendored corpus is used. The corpus is written in the suite's own idiom so that
/// pointing this at a real checkout changes only how many tests there are, never how they run.
pub const SUITES: &str = "ZGUI_WPT";

/// What happened to one test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The subject and the reference laid out identically.
    Pass,
    /// They did not, with the first difference.
    Fail(String),
    /// The markup is outside the subset the converter accepts.
    Unconvertible(String),
}

/// One test's name and what happened to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestResult {
    /// The file name, without its directory.
    pub name: String,
    /// What happened.
    pub outcome: Outcome,
}

/// One suite's results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuiteResult {
    /// The suite's directory name.
    pub name: String,
    /// How many tests it holds, including the unconvertible ones.
    pub tests: usize,
    /// How many passed.
    pub passing: usize,
    /// How many could not be converted.
    pub unconvertible: usize,
    /// Every test, in name order.
    pub results: Vec<TestResult>,
}

impl SuiteResult {
    /// The share of this suite that passes, between zero and one.
    ///
    /// Unconvertible tests are in the denominator on purpose. A rate that ignored them would rise
    /// whenever the converter started refusing something, which is the opposite of what it should
    /// do.
    pub fn rate(&self) -> f64 {
        if self.tests == 0 {
            return 0.0;
        }
        self.passing as f64 / self.tests as f64
    }
}

/// Where the suites are.
pub fn root() -> PathBuf {
    std::env::var_os(SUITES).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("suites"),
        PathBuf::from,
    )
}

/// Runs every suite under [`root`], in name order.
///
/// # Errors
///
/// Returns a message when the directory cannot be read. Never an empty result: a suite runner that
/// found nothing and reported success would be a ratchet holding up nothing.
pub fn run_all() -> Result<Vec<SuiteResult>, String> {
    let root = root();
    let mut directories: Vec<PathBuf> = std::fs::read_dir(&root)
        .map_err(|error| format!("{}: {error}", root.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    directories.sort();
    if directories.is_empty() {
        return Err(format!("{} holds no suites", root.display()));
    }
    directories.iter().map(|path| run(path)).collect()
}

/// Runs one suite directory.
///
/// # Errors
///
/// Returns a message when the directory cannot be read or holds no tests.
pub fn run(directory: &Path) -> Result<SuiteResult, String> {
    let name = directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut files: Vec<PathBuf> = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "html")
                && !path
                    .file_stem()
                    .is_some_and(|stem| stem.to_string_lossy().ends_with("-ref"))
        })
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("{} holds no tests", directory.display()));
    }

    let results: Vec<TestResult> = files
        .iter()
        .map(|path| TestResult {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            outcome: run_one(path),
        })
        .collect();
    Ok(SuiteResult {
        name,
        tests: results.len(),
        passing: results
            .iter()
            .filter(|result| result.outcome == Outcome::Pass)
            .count(),
        unconvertible: results
            .iter()
            .filter(|result| matches!(result.outcome, Outcome::Unconvertible(_)))
            .count(),
        results,
    })
}

/// Converts one test and its reference and compares the two fragment trees.
fn run_one(path: &Path) -> Outcome {
    let subject = match document(path) {
        Ok(document) => document,
        Err(reason) => return Outcome::Unconvertible(reason),
    };
    let Some(reference_name) = subject.1 else {
        return Outcome::Unconvertible("the test names no reference".to_owned());
    };
    let reference_path = path.with_file_name(reference_name);
    let reference = match document(&reference_path) {
        Ok(document) => document.0,
        Err(reason) => return Outcome::Unconvertible(reason),
    };

    let left = fragment::project(&lay_out(&subject.0).store);
    let right = fragment::project(&lay_out(&reference).store);
    if left == right {
        return Outcome::Pass;
    }
    Outcome::Fail(difference(&left, &right))
}

/// Converts one file into a document and the reference it names.
fn document(path: &Path) -> Result<(Zdoc, Option<String>), String> {
    let source =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let converted = markup::convert(&source).map_err(|error| error.to_string())?;
    Ok((
        Zdoc {
            viewport: Zdoc::DEFAULT_VIEWPORT,
            css: converted.css,
            root: converted.root,
        },
        converted.reference,
    ))
}

/// The first line the two renderings differ on.
fn difference(left: &str, right: &str) -> String {
    for (number, (a, b)) in left.lines().zip(right.lines()).enumerate() {
        if a != b {
            return format!("line {}:\n  test      {a}\n  reference {b}", number + 1);
        }
    }
    format!(
        "the test has {} fragments and the reference {}",
        left.lines().count(),
        right.lines().count(),
    )
}

#[cfg(test)]
mod tests {
    use super::{Outcome, run_all};

    /// Every vendored suite converts and every test in it passes.
    #[test]
    fn the_whole_corpus_passes() {
        let suites = run_all().expect("the corpus is readable");
        assert!(suites.len() >= 5, "{} suites", suites.len());
        for suite in &suites {
            for result in &suite.results {
                assert_eq!(
                    result.outcome,
                    Outcome::Pass,
                    "{}/{}",
                    suite.name,
                    result.name,
                );
            }
        }
    }

    /// The runner can fail, and the failure names the fragment that moved.
    ///
    /// Without this the check above would pass identically against a runner that compared nothing.
    #[test]
    fn a_test_that_does_not_match_its_reference_fails() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("counter-suite");
        let suite = super::run(&directory).expect("the counter suite is readable");
        assert_eq!(suite.tests, 2);
        assert_eq!(suite.passing, 0);
        assert_eq!(suite.unconvertible, 1);
        let failed = suite
            .results
            .iter()
            .find(|result| matches!(result.outcome, Outcome::Fail(_)))
            .expect("one test differs from its reference");
        assert!(matches!(&failed.outcome, Outcome::Fail(report) if report.contains("reference")));
        assert!((suite.rate() - 0.0).abs() < f64::EPSILON);
    }
}
