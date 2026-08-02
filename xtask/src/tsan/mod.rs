//! The thread sanitiser run, and the control that makes its silence mean something.
//!
//! The style traversal hands one element to each of six workers and every worker walks outwards
//! from its own, reading records other workers are standing on and writing selector flags onto
//! shared parents. Nothing else in the gate set can see a defect there: an interpreter runs one
//! thread, and a suite that passes proves only that this particular interleaving was fine.
//!
//! So this points ThreadSanitizer at the targets where those threads meet — and, in the same
//! invocation, at a deliberate data race it must report. Without that control a run in which the
//! instrumentation never reached the code, or in which the suppression file was written too wide,
//! comes back silent and green and is indistinguishable from a run that found nothing to say.

mod invoke;
mod subject;
mod verdict;

use std::path::Path;

use crate::error::{Error, Result};
use crate::tsan::invoke::Session;
use crate::tsan::subject::{Expectation, SUBJECTS, Subject};
use crate::tsan::verdict::Verdict;

/// Runs every sanitised target and reports what the sanitiser said about each.
pub(crate) fn run(root: &Path) -> Result<()> {
    let session = Session::open(root)?;
    let mut failures = Vec::new();

    for subject in SUBJECTS {
        println!("\n=== tsan {} ===\n{}", subject.label(), subject.purpose);
        session.build(subject)?;
        let run = session.run(subject)?;
        let verdict = Verdict::of(&run.output);
        println!("tsan {}: {}", subject.label(), verdict.summary());
        if let Some(failure) = judge(subject, verdict, run.finished) {
            failures.push(failure);
        }
    }

    if failures.is_empty() {
        println!(
            "\ntsan: {} target(s) clean, and the control fired",
            SUBJECTS.len() - 1
        );
        Ok(())
    } else {
        Err(Error::failed(format!(
            "the thread sanitiser run found {} problem(s):\n    {}",
            failures.len(),
            failures.join("\n    ")
        )))
    }
}

/// What is wrong with one run, if anything.
///
/// `finished` is whether the target ran to completion with its tests passing, which is asked before
/// anything the sanitiser said: a target that panicked partway through, or whose process died
/// before the runtime was mapped, prints no report about the code it never reached, and reading
/// that silence as cleanliness is the exact mistake the control exists to prevent.
fn judge(subject: &Subject, verdict: Verdict, finished: bool) -> Option<String> {
    if verdict.suppressions_unreadable {
        return Some(format!(
            "{}: the sanitiser could not read {}, so it ran with no suppressions at all and \
             whatever it reported is about the wrong configuration",
            subject.label(),
            invoke::SUPPRESSIONS
        ));
    }
    if !finished {
        return Some(format!(
            "{}: the target exited unsuccessfully, so its tests did not all pass and the code the \
             sanitiser was pointed at did not all run. Whatever it reported — including nothing — \
             is about a partial execution. Read the output above for the failure.",
            subject.label()
        ));
    }
    match subject.expectation {
        Expectation::Silent if verdict.is_clean() => None,
        Expectation::Silent => Some(format!("{}: {}", subject.label(), verdict.summary())),
        Expectation::Race if verdict.races > 0 => None,
        Expectation::Race => Some(format!(
            "{}: the positive control reported no data race. The sanitiser was not watching, so \
             every other target in this run is silent for a reason nobody has established. Check \
             that `-Zsanitizer=thread` reached the build and that the suppression file has not been \
             widened to cover the control.",
            subject.label()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Subject, Verdict, judge};
    use crate::tsan::subject::Expectation;

    /// A subject with the given expectation.
    fn subject(expectation: Expectation) -> Subject {
        Subject {
            package: "zgui-dom",
            test: "example",
            purpose: "a test",
            expectation,
        }
    }

    /// A verdict with `races` data races and nothing else.
    fn saw(races: usize) -> Verdict {
        Verdict {
            races,
            ..Verdict::default()
        }
    }

    #[test]
    fn a_silent_target_passes_when_silent_and_fails_when_it_speaks() {
        assert!(judge(&subject(Expectation::Silent), saw(0), true).is_none());
        let failure =
            judge(&subject(Expectation::Silent), saw(1), true).expect("a race is a failure");
        assert!(failure.contains("1 data race"));
    }

    #[test]
    fn the_control_fails_the_run_when_it_does_not_fire() {
        // The whole point: a control that reports nothing means the run proved nothing, so it has
        // to be louder than a clean target, not quieter.
        let failure = judge(&subject(Expectation::Race), saw(0), true)
            .expect("a silent control is a failure");
        assert!(failure.contains("not watching"));
        assert!(judge(&subject(Expectation::Race), saw(2), true).is_none());
    }

    #[test]
    fn a_target_that_did_not_finish_fails_however_quiet_it_was() {
        // A panicking target prints no report about the code after the panic, and a process that
        // died before the runtime was mapped prints none at all. Both come back with nothing to
        // say, which is indistinguishable from cleanliness unless the exit status is read.
        let failure = judge(&subject(Expectation::Silent), saw(0), false)
            .expect("an unfinished target is a failure");
        assert!(failure.contains("did not all run"));
    }

    #[test]
    fn a_control_that_fired_but_did_not_finish_still_fails() {
        // The control firing says the sanitiser was watching. It says nothing about whether the
        // target it was watching got to the end, so it cannot excuse a failed run.
        let failure = judge(&subject(Expectation::Race), saw(1), false)
            .expect("an unfinished control is a failure");
        assert!(failure.contains("exited unsuccessfully"));
    }

    #[test]
    fn an_unreadable_suppression_file_fails_even_a_control_that_fired() {
        // Suppressions that did not load make a run stricter than the one that was configured, so
        // its result is about a configuration nobody chose — including when the control fired.
        let verdict = Verdict {
            races: 3,
            suppressions_unreadable: true,
            ..Verdict::default()
        };
        let failure =
            judge(&subject(Expectation::Race), verdict, true).expect("a misconfigured run");
        assert!(failure.contains("could not read"));
    }
}
