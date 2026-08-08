//! Running every probe once, and the two contradictions that answer settles.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use zgui_css::parity::Support;

use crate::census::Census;
use crate::evidence::probe::Verdict;
use crate::evidence::{probes, timed, unproven};

/// What every probe showed, taken once per process.
///
/// Taking it once matters: a probe lays two documents out, and asking three hundred questions of
/// the answer would otherwise lay six hundred documents out per question.
pub struct Survey {
    /// One verdict per longhand, by the name a style sheet writes.
    verdicts: BTreeMap<String, Verdict>,
}

impl Survey {
    /// Runs every probe, or returns the run already taken.
    pub fn take() -> &'static Self {
        static SURVEY: OnceLock<Survey> = OnceLock::new();
        SURVEY.get_or_init(|| {
            zgui_css::enable_css_features();
            let baseline = crate::evidence::probe::Baseline::take();
            let mut verdicts: BTreeMap<String, Verdict> = probes::all()
                .iter()
                .map(|probe| (probe.css_name(), probe.run_against(&baseline)))
                .collect();
            // A property that describes change over time is asked twice, and the stronger answer is
            // the true one. The static instrument lays a document out at a single moment, which is
            // an animation at time zero — so it can only ever report the whole motion vocabulary as
            // having no effect, and its silence is the instrument's rather than the framework's.
            for (css_name, verdict) in timed::survey() {
                if verdict != Verdict::Unchanged {
                    verdicts.insert(css_name, verdict);
                }
            }
            Self { verdicts }
        })
    }

    /// What one property's probe showed.
    pub fn verdict(&self, css_name: &str) -> Option<Verdict> {
        self.verdicts.get(css_name).copied()
    }

    /// Every property whose probe never reached a computed style.
    ///
    /// Always a fault in the probe. It is reported separately from the findings because a broken
    /// probe proves nothing in either direction, and counting it as "no effect" would quietly turn
    /// a typo into a parity claim.
    pub fn inert(&self) -> Vec<&str> {
        self.with(Verdict::Inert)
    }

    /// Every property whose value visibly changed what layout produced.
    pub fn proven(&self) -> Vec<&str> {
        self.with(Verdict::Changed)
    }

    /// Every property the cascade took and nothing downstream acted on.
    pub fn inconsequential(&self) -> Vec<&str> {
        self.with(Verdict::Unchanged)
    }

    /// The properties whose probe gave `wanted`.
    fn with(&self, wanted: Verdict) -> Vec<&str> {
        self.verdicts
            .iter()
            .filter(|(_, verdict)| **verdict == wanted)
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

/// A register row and what the probes say about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The property, as a style sheet writes it.
    pub css_name: String,
    /// What the row claims.
    pub claimed: Support,
    /// What running the framework showed.
    pub verdict: Verdict,
}

/// Every row whose claim the probes contradict.
///
/// Two shapes, and they are the two ways a parity number goes wrong:
///
/// * a property declared **implemented** whose probe changed nothing — the over-claim, and the
///   defect review has caught in this very register;
/// * a property declared **unread** whose probe changed the fragment tree — the under-claim, which
///   reaches an author as a feature that works and a number that denies it.
///
/// A row in the first shape is allowed exactly one escape: naming itself in [`unproven`] with the
/// reason no probe in this build can reach it. That escape is a visible list, not a silence.
pub fn contradictions(census: &Census, survey: &Survey) -> Vec<Finding> {
    let mut out = Vec::new();
    for css_name in census.canonical() {
        let (Some(claimed), Some(verdict)) = (census.answer(css_name), survey.verdict(css_name))
        else {
            continue;
        };
        let wrong = match (claimed, verdict) {
            (_, Verdict::Inert) => true,
            (Support::Implemented(_), Verdict::Unchanged) => unproven::reason(css_name).is_none(),
            (Support::Ignored(_), Verdict::Changed) => true,
            _ => false,
        };
        if wrong {
            out.push(Finding {
                css_name: css_name.clone(),
                claimed,
                verdict,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Survey, contradictions};
    use crate::census::Census;

    /// No probe is broken.
    #[test]
    fn every_probe_reaches_a_computed_style() {
        let survey = Survey::take();
        assert_eq!(survey.inert(), Vec::<&str>::new());
    }

    /// No register row anywhere in the workspace is contradicted by running the framework.
    ///
    /// This is the assertion the whole crate exists for: it is what makes a parity number a
    /// measurement rather than a claim.
    #[test]
    fn no_declaration_is_contradicted_by_what_the_framework_does() {
        let findings = contradictions(&Census::take(), Survey::take());
        assert_eq!(findings, Vec::new(), "{findings:#?}");
    }

    /// The survey found effects and non-effects, and neither set is everything.
    ///
    /// Without this the check above would pass just as happily against a probe runner that had
    /// stopped working and answered the same thing to every question.
    #[test]
    fn the_survey_discriminates() {
        let survey = Survey::take();
        assert!(survey.proven().len() > 40, "{}", survey.proven().len());
        assert!(
            survey.inconsequential().len() > 40,
            "{}",
            survey.inconsequential().len(),
        );
    }
}
