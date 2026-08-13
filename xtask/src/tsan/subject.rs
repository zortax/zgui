//! What the sanitiser is pointed at, and what it must say about each target.

/// What a sanitised run has to report to count as a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expectation {
    /// The sanitiser must report nothing.
    Silent,
    /// The sanitiser must report at least one data race.
    ///
    /// This is the positive control. A run of the whole set in which this target comes back clean
    /// is a run in which the sanitiser was not watching — a stale binary, instrumentation that
    /// never reached the crate, a suppression written far too wide — and every silent target in the
    /// same set means nothing. Which is why it is a failure rather than a curiosity.
    Race,
}

/// One test binary, run under the sanitiser.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Subject {
    /// The package the test target belongs to.
    pub(crate) package: &'static str,
    /// The integration test target's name.
    pub(crate) test: &'static str,
    /// What this run is evidence about, printed with its verdict.
    pub(crate) purpose: &'static str,
    /// What the sanitiser has to say for the run to pass.
    pub(crate) expectation: Expectation,
}

/// The variable that arms the deliberate race inside the control target.
///
/// It is set for the control run and for nothing else, so an ordinary `cargo test` never executes
/// the race.
pub(crate) const CONTROL_VARIABLE: &str = "ZGUI_SANITIZER_CONTROL";

/// Every target the sanitiser runs, control first.
///
/// The control comes first so that a configuration in which the sanitiser is not actually watching
/// fails before the long runs are spent, rather than after them and only if someone reads the log.
pub(crate) const SUBJECTS: &[Subject] = &[
    Subject {
        package: "zgui-dom",
        test: "sanitizer_control",
        purpose: "the positive control: the selector-flag write done without an atomic",
        expectation: Expectation::Race,
    },
    Subject {
        package: "zgui-dom",
        test: "workers",
        purpose: "the cell discipline read and written by many threads over one document",
        expectation: Expectation::Silent,
    },
    Subject {
        package: "zgui-dom",
        test: "shared",
        purpose: "every shared-borrow store in the document, driven from two threads",
        expectation: Expectation::Silent,
    },
    Subject {
        package: "zgui-style",
        test: "restyle",
        purpose: "the parallel style traversal at every worker count the engine supports",
        expectation: Expectation::Silent,
    },
    Subject {
        package: "zgui-text-parley",
        test: "parallel_shape",
        purpose: "forked shapers on worker threads over one shared font system",
        expectation: Expectation::Silent,
    },
    Subject {
        package: "zgui-layout",
        test: "parallel_layout",
        purpose: "layout batch workers over one shared store, at several pool widths",
        expectation: Expectation::Silent,
    },
];

impl Subject {
    /// Whether this run arms the deliberate race.
    pub(crate) fn arms_control(&self) -> bool {
        self.expectation == Expectation::Race
    }

    /// How the target is named in a log line.
    pub(crate) fn label(&self) -> String {
        format!("{} --test {}", self.package, self.test)
    }
}

#[cfg(test)]
mod tests {
    use super::{Expectation, SUBJECTS};

    #[test]
    fn the_control_runs_before_anything_it_is_evidence_about() {
        assert_eq!(
            SUBJECTS[0].expectation,
            Expectation::Race,
            "the control has to be the first target, or a broken configuration is only \
             discovered after every clean run has already been believed"
        );
    }

    #[test]
    fn there_is_exactly_one_control_and_something_for_it_to_be_evidence_about() {
        let controls = SUBJECTS
            .iter()
            .filter(|subject| subject.expectation == Expectation::Race)
            .count();
        assert_eq!(controls, 1);
        assert!(
            SUBJECTS.len() > controls,
            "a run consisting only of its own control measures nothing"
        );
    }
}
