//! Reading a sanitiser's mind from what it printed.
//!
//! ThreadSanitizer reports through its output rather than through an exit status that distinguishes
//! a race from a failed assertion, so the verdict on a run is a property of its text.

/// The prefix every ThreadSanitizer report line starts with.
const WARNING: &str = "WARNING: ThreadSanitizer: ";

/// The report class that means two accesses were unsynchronised.
const DATA_RACE: &str = "data race";

/// The line printed for each suppression that matched, when `print_suppressions` is on.
const SUPPRESSED: &str = "ThreadSanitizer: Matched ";

/// The line the sanitiser prints when its suppression file cannot be read.
///
/// A path that does not resolve is the failure mode a suppressed run must never survive: the
/// suppressions silently do not apply, the engine's own reports come back, and the natural response
/// is to widen the file rather than to fix the path.
const BAD_SUPPRESSIONS: &str = "failed to read suppressions file";

/// What one sanitised run reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Verdict {
    /// How many data races were reported.
    pub(crate) races: usize,
    /// How many reports of any other class were made, such as a thread leak.
    pub(crate) other_warnings: usize,
    /// How many reports a suppression swallowed.
    pub(crate) suppressed: usize,
    /// Whether the sanitiser could not read the suppression file it was given.
    pub(crate) suppressions_unreadable: bool,
}

impl Verdict {
    /// Reads the verdict out of a run's combined output.
    pub(crate) fn of(output: &str) -> Self {
        let mut verdict = Self::default();
        for line in output.lines() {
            if let Some(rest) = after(line, WARNING) {
                if rest.starts_with(DATA_RACE) {
                    verdict.races += 1;
                } else {
                    verdict.other_warnings += 1;
                }
            }
            if let Some(rest) = after(line, SUPPRESSED) {
                verdict.suppressed += count(rest);
            }
            if line.contains(BAD_SUPPRESSIONS) {
                verdict.suppressions_unreadable = true;
            }
        }
        verdict
    }

    /// Whether the sanitiser objected to anything.
    pub(crate) fn is_clean(self) -> bool {
        self.races == 0 && self.other_warnings == 0
    }

    /// A one-line summary for the log.
    pub(crate) fn summary(self) -> String {
        format!(
            "{} data race(s), {} other warning(s), {} report(s) suppressed",
            self.races, self.other_warnings, self.suppressed
        )
    }
}

/// The remainder of `line` after `marker`, when the line holds it.
fn after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.find(marker).map(|at| &line[at + marker.len()..])
}

/// The leading integer of `text`, or one when it has none.
///
/// `Matched 3 suppressions` is a count; a future wording without one still means at least one
/// report was hidden, and reporting zero there would understate what the file cost the run.
fn count(text: &str) -> usize {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::Verdict;

    #[test]
    fn a_silent_run_is_clean() {
        let verdict = Verdict::of("running 3 tests\ntest result: ok. 3 passed\n");
        assert_eq!(verdict, Verdict::default());
        assert!(verdict.is_clean());
    }

    #[test]
    fn data_races_are_counted_and_separated_from_other_report_classes() {
        let output = "\
==================\n\
WARNING: ThreadSanitizer: data race (pid=1)\n\
  Write of size 4 at 0x7b0400000010 by thread T2:\n\
==================\n\
WARNING: ThreadSanitizer: data race (pid=1)\n\
==================\n\
WARNING: ThreadSanitizer: thread leak (pid=1)\n";
        let verdict = Verdict::of(output);
        assert_eq!(verdict.races, 2);
        assert_eq!(verdict.other_warnings, 1);
        assert!(!verdict.is_clean());
    }

    #[test]
    fn suppressed_reports_are_counted_from_the_number_in_the_line() {
        let verdict = Verdict::of("ThreadSanitizer: Matched 3 suppressions (pid=1):\n");
        assert_eq!(verdict.suppressed, 3);
        assert!(
            verdict.is_clean(),
            "a suppressed report is not a warning against this workspace"
        );
    }

    #[test]
    fn an_unreadable_suppression_file_is_noticed() {
        // The sanitiser says this and carries on with no suppressions at all, which is the one
        // failure that makes a run look stricter than it is rather than weaker.
        let verdict =
            Verdict::of("ThreadSanitizer: failed to read suppressions file '/nope/here.txt'\n");
        assert!(verdict.suppressions_unreadable);
    }

    #[test]
    fn the_summary_names_all_three_numbers() {
        let verdict = Verdict::of(
            "WARNING: ThreadSanitizer: data race\nThreadSanitizer: Matched 2 suppressions\n",
        );
        assert_eq!(
            verdict.summary(),
            "1 data race(s), 0 other warning(s), 2 report(s) suppressed"
        );
    }
}
