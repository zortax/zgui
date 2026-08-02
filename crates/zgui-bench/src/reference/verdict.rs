//! What a reference workload is allowed to measure, and what it says when it did not.
//!
//! Every gate here is a **dimensionless number**: a ratio of two quantities taken in the same
//! process, over the same documents, minutes apart at most. A machine twice as fast divides both
//! halves by the same number and leaves the ratio exactly where it was, which is what makes a
//! recorded value comparable at all. A gate stated in microseconds can only ever be made to pass
//! again by recording it again, and a gate that is re-recorded whenever it fires gates nothing.
//!
//! The second thing this module exists for is the failure a gate acquires silently. A workload's
//! measurement stops working — the document stopped being built, the gesture stopped reaching it,
//! the sizes stopped differing — and every slope comes back `None`. A criterion written as "not
//! worse than" holds against nothing at all and the gate stays green for ever. So a missing number
//! is [`Broken`], and broken fails.

use core::fmt;

/// What a dimensionless number is allowed to be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Allowed {
    /// Within `tolerance` of `recorded`, as a fraction of `recorded`.
    ///
    /// For a ratio with a value worth stating: a resize that costs a little under the pipeline
    /// underneath it, a damage fraction that is what a row of a list covers. Two-sided on purpose —
    /// a ratio that fell is a measurement that changed as much as one that rose, and the half that
    /// moved may be the denominator.
    Near {
        /// What the ratio was when the gate was written.
        recorded: f64,
        /// How far either way it may land, as a fraction of `recorded`.
        tolerance: f64,
    },
    /// No greater than `most`.
    ///
    /// For a ratio whose *ideal* is zero and whose failure is one: what a single-property update
    /// costs against what a whole-document update costs, what a hundred thousand rows cost against
    /// what twelve thousand five hundred cost. There is nothing to record two-sidedly, because
    /// smaller is the win the workload exists to protect.
    Under {
        /// The largest value that is not a regression.
        most: f64,
    },
}

impl Allowed {
    /// Whether `ratio` is inside.
    #[must_use]
    pub fn admits(self, ratio: f64) -> bool {
        match self {
            Self::Near {
                recorded,
                tolerance,
            } => (ratio - recorded).abs() <= recorded * tolerance,
            Self::Under { most } => ratio <= most,
        }
    }
}

impl fmt::Display for Allowed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Near {
                recorded,
                tolerance,
            } => write!(
                formatter,
                "recorded {recorded:.4} +/-{:.0}%",
                tolerance * 100.0
            ),
            Self::Under { most } => write!(formatter, "at most {most:.4}"),
        }
    }
}

/// Why there is no ratio to judge.
///
/// Each of these is a broken measurement rather than a regression, and the message says so: a
/// person who reads "REGRESSION" goes looking for the commit that caused it, and a person who
/// reads this needs to go looking for the workload that stopped working.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Broken {
    /// The quantity being judged has no value — its sizes did not determine a line.
    NoSubject,
    /// The same-run baseline has no value.
    NoBaseline,
    /// The same-run baseline came out at zero or below, so there is nothing to state a ratio
    /// against.
    BaselineIsZero,
}

impl fmt::Display for Broken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let said = match self {
            Self::NoSubject => {
                "the sizes did not determine a line, so the quantity this gate is about was never \
                 measured"
            }
            Self::NoBaseline => {
                "the same-run baseline did not determine a line, so there is nothing to state the \
                 measurement against"
            }
            Self::BaselineIsZero => {
                "the same-run baseline came out at zero, so either the documents are too small for \
                 it to reach or it stopped reaching them"
            }
        };
        formatter.write_str(said)
    }
}

/// One gate: a dimensionless number, what it is the ratio of, and what it may be.
pub struct Criterion {
    /// What the gate is called, in the line a run prints and in the message a failure names.
    pub name: &'static str,
    /// What the numerator is, in words a person can act on.
    pub subject: &'static str,
    /// What the denominator is — the same-run baseline.
    pub baseline: &'static str,
    /// The band.
    pub allowed: Allowed,
    /// What to look at when it moves.
    pub advice: &'static str,
}

impl Criterion {
    /// Judges `subject` against `baseline`, both of which may be missing.
    #[must_use]
    pub fn judge(&self, subject: Option<f64>, baseline: Option<f64>) -> Judged<'_> {
        let ratio = match (subject, baseline) {
            (None, _) => Err(Broken::NoSubject),
            (_, None) => Err(Broken::NoBaseline),
            (Some(_), Some(baseline)) if baseline <= 0.0 => Err(Broken::BaselineIsZero),
            (Some(subject), Some(baseline)) => Ok(subject / baseline),
        };
        Judged {
            criterion: self,
            ratio,
        }
    }

    /// Judges a number that is already dimensionless — a fraction of a surface, a count per tick —
    /// with no division to do.
    ///
    /// The same refusal applies: `None` is a measurement that did not happen.
    #[must_use]
    pub fn judge_directly(&self, measured: Option<f64>) -> Judged<'_> {
        Judged {
            criterion: self,
            ratio: measured.ok_or(Broken::NoSubject),
        }
    }
}

/// What one criterion came to in one run.
pub struct Judged<'a> {
    /// The gate.
    criterion: &'a Criterion,
    /// The ratio, or why there is not one.
    ratio: Result<f64, Broken>,
}

impl Judged<'_> {
    /// Whether the run is inside the band.
    ///
    /// A broken measurement is not inside it. That is the whole point of the type.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.ratio
            .is_ok_and(|ratio| self.criterion.allowed.admits(ratio))
    }
}

impl fmt::Display for Judged<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.criterion.name;
        match self.ratio {
            Err(broken) => write!(
                formatter,
                "{name} BROKEN: {broken}. This is a broken measurement rather than a regression: \
                 the workload stopped measuring {}.",
                self.criterion.subject
            ),
            Ok(ratio) if self.criterion.allowed.admits(ratio) => write!(
                formatter,
                "{name} ok  {ratio:.4}  ({})",
                self.criterion.allowed
            ),
            Ok(ratio) => write!(
                formatter,
                "{name} REGRESSION  {ratio:.4}, {}. {} is now {ratio:.4} times {}, over the same \
                 documents in this same run. {}",
                self.criterion.allowed,
                self.criterion.subject,
                self.criterion.baseline,
                self.criterion.advice
            ),
        }
    }
}

/// Every criterion a workload states, and the advisory numbers beside them.
#[derive(Default)]
pub struct Report {
    /// The lines that gate nothing: slopes in real units, keyed to the machine that took them.
    advisory: Vec<String>,
    /// The verdicts.
    verdicts: Vec<String>,
    /// How many of them failed.
    failed: usize,
    /// Whether any criterion was judged at all.
    judged: usize,
}

impl Report {
    /// A report with nothing in it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a number that gates nothing — a slope in microseconds per box, a count, a size.
    ///
    /// Printed with the word `advisory` on it, so that a number keyed to one machine cannot be read
    /// off a log as a threshold somebody else's machine failed.
    pub fn advisory(&mut self, line: impl Into<String>) {
        self.advisory.push(line.into());
    }

    /// Records a verdict.
    pub fn judged(&mut self, judged: &Judged<'_>) {
        self.judged += 1;
        if !judged.passed() {
            self.failed += 1;
        }
        self.verdicts.push(judged.to_string());
    }

    /// Whether every criterion passed **and** there was at least one.
    ///
    /// The second half is not pedantry. A workload whose sweep loop stopped running reports no
    /// verdicts, and "none of them failed" is true of no verdicts at all.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.failed == 0 && self.judged > 0
    }
}

impl fmt::Display for Report {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in &self.advisory {
            writeln!(
                formatter,
                "  advisory (keyed to this machine, gating nothing)  {line}"
            )?;
        }
        for line in &self.verdicts {
            writeln!(formatter, "{line}")?;
        }
        if self.judged == 0 {
            write!(
                formatter,
                "BROKEN: the workload stated no criterion at all, so nothing about it was checked."
            )
        } else if self.failed == 0 {
            write!(formatter, "ok: {} criteria, all inside", self.judged)
        } else {
            write!(
                formatter,
                "FAILED: {} of {} criteria outside",
                self.failed, self.judged
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Allowed, Broken, Criterion, Report};

    /// A criterion with a two-sided band, as a resize-shaped gate has.
    const NEAR: Criterion = Criterion {
        name: "TEST-near",
        subject: "the thing measured",
        baseline: "the thing it is measured against",
        allowed: Allowed::Near {
            recorded: 0.80,
            tolerance: 0.10,
        },
        advice: "look at the first half.",
    };

    /// A criterion with a ceiling, as a virtualisation-shaped gate has.
    const UNDER: Criterion = Criterion {
        name: "TEST-under",
        subject: "what a local update costs",
        baseline: "what a whole-document update costs",
        allowed: Allowed::Under { most: 0.05 },
        advice: "look for work proportional to the document.",
    };

    #[test]
    fn a_ratio_in_the_band_passes_and_one_outside_it_does_not() {
        assert!(NEAR.judge(Some(8.0), Some(10.0)).passed());
        assert!(!NEAR.judge(Some(9.5), Some(10.0)).passed());
        // Two-sided: a ratio that fell out of the band is as much a change as one that rose.
        assert!(!NEAR.judge(Some(6.0), Some(10.0)).passed());
    }

    #[test]
    fn a_ceiling_is_one_sided() {
        assert!(UNDER.judge(Some(0.0), Some(10.0)).passed());
        assert!(UNDER.judge(Some(0.5), Some(10.0)).passed());
        assert!(!UNDER.judge(Some(1.0), Some(10.0)).passed());
    }

    #[test]
    fn a_measurement_that_did_not_happen_fails_rather_than_passing_quietly() {
        // The failure mode the type exists for. Under a ceiling especially: `None` read as zero
        // would sail through `at most 0.05` for ever.
        let broken = UNDER.judge(None, Some(10.0));
        assert!(!broken.passed());
        assert!(broken.to_string().contains("BROKEN"), "{broken}");
        assert!(
            broken
                .to_string()
                .contains("broken measurement rather than"),
            "{broken}"
        );
    }

    #[test]
    fn a_baseline_that_did_not_happen_says_so_rather_than_dividing_by_it() {
        assert!(!NEAR.judge(Some(1.0), None).passed());
        assert!(!NEAR.judge(Some(1.0), Some(0.0)).passed());
        let message = NEAR.judge(Some(1.0), Some(0.0)).to_string();
        assert!(
            message.contains(&Broken::BaselineIsZero.to_string()),
            "{message}"
        );
    }

    #[test]
    fn a_direct_number_is_judged_without_a_division() {
        // Damage fractions and per-tick counts are dimensionless before anybody divides them.
        assert!(UNDER.judge_directly(Some(0.01)).passed());
        assert!(!UNDER.judge_directly(Some(0.9)).passed());
        assert!(!UNDER.judge_directly(None).passed());
    }

    #[test]
    fn a_failure_names_the_two_halves_and_says_what_to_look_at() {
        let message = UNDER.judge(Some(9.0), Some(10.0)).to_string();
        assert!(message.contains("REGRESSION"), "{message}");
        assert!(message.contains("what a local update costs"), "{message}");
        assert!(
            message.contains("what a whole-document update costs"),
            "{message}"
        );
        assert!(
            message.contains("proportional to the document"),
            "{message}"
        );
    }

    #[test]
    fn a_report_with_no_criteria_in_it_has_not_passed() {
        // The vacuity a workload acquires when its sweep stops running: nothing failed, because
        // nothing was checked.
        let report = Report::new();
        assert!(!report.passed());
        assert!(report.to_string().contains("stated no criterion at all"));
    }

    #[test]
    fn a_report_counts_what_failed() {
        let mut report = Report::new();
        report.advisory("slope=1.2345 us/box");
        report.judged(&NEAR.judge(Some(8.0), Some(10.0)));
        assert!(report.passed());
        report.judged(&UNDER.judge(Some(9.0), Some(10.0)));
        assert!(!report.passed());
        let printed = report.to_string();
        assert!(printed.contains("gating nothing"), "{printed}");
        assert!(printed.contains("FAILED: 1 of 2"), "{printed}");
    }
}
