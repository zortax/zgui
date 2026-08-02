//! What the two slopes have to be, and what to say when they are not.

use std::fmt;

use super::fit;

/// The recorded ratio of the two slopes.
///
/// Dimensionless: microseconds per box for a configure, over microseconds per box for a change to
/// the document's own content that forces the same restyle, relayout and full repaint. A configure
/// that costs the pipeline underneath it and nothing more sits a little under one: it reaches the
/// same layout and the same repaint without owing the cascade a class change owes. Recorded, but
/// **not** a duration recorded from a run: a faster machine divides both slopes by the same number
/// and leaves this exactly where it is, which is what makes it comparable at all.
const RECORDED: f64 = 0.865;

/// How far the ratio may move before it is a regression.
///
/// Ten per cent, as the gate is written. Both slopes are least-squares fits over four medians of
/// forty-eight samples each, and the two are taken minutes apart at most on the same documents in
/// the same process, so the machine's own drift is common to both and divides out.
const TOLERANCE: f64 = 0.10;

/// What one run found.
pub(crate) struct Verdict {
    /// Microseconds per box for a configure.
    resize: Option<f64>,
    /// Microseconds per box for the whole-document content change.
    content: Option<f64>,
}

impl Verdict {
    /// Fits both slopes and judges their ratio.
    pub(crate) fn of(resizes: &[(f64, f64)], contents: &[(f64, f64)]) -> Self {
        Self {
            resize: fit::slope(resizes),
            content: fit::slope(contents),
        }
    }

    /// The ratio, when both slopes exist and the baseline is not zero.
    fn ratio(&self) -> Option<f64> {
        let (resize, content) = (self.resize?, self.content?);
        (content > 0.0).then_some(resize / content)
    }

    /// Whether the run is inside the band.
    pub(crate) fn passed(&self) -> bool {
        self.ratio()
            .is_some_and(|ratio| (ratio - RECORDED).abs() <= RECORDED * TOLERANCE)
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Some(resize), Some(content)) = (self.resize, self.content) else {
            return write!(
                formatter,
                "RESIZE FAILED: the document sizes did not determine a line, so no slope was \
                 measured. This is a broken measurement rather than a regression."
            );
        };
        writeln!(
            formatter,
            "RESIZE slope resize={resize:.4} us/box content={content:.4} us/box  \
             (advisory, keyed to this machine, gating nothing)"
        )?;
        let Some(ratio) = self.ratio() else {
            return write!(
                formatter,
                "RESIZE FAILED: the content change cost nothing per box, so there is no same-run \
                 baseline to state the resize slope against. Either the document is too small for \
                 the change to reach or the change stopped reaching it."
            );
        };
        if self.passed() {
            write!(
                formatter,
                "RESIZE ok  ratio={ratio:.4} of the same-run baseline, recorded {RECORDED:.4} \
                 +/-{:.0}%",
                TOLERANCE * 100.0
            )
        } else {
            write!(
                formatter,
                "RESIZE REGRESSION  ratio={ratio:.4}, recorded {RECORDED:.4} +/-{:.0}%. A \
                 configure now costs {ratio:.2} times what the same restyle, relayout and repaint \
                 costs when a content change asks for it, over the same four documents in this \
                 same run. {}\nBoth slopes are in the line above: the one that moved is the one to \
                 look at. Resize is out of the compositor programme's scope, so this is a gate \
                 against regressing it by accident — if the change was deliberate, move `RECORDED` \
                 in `crates/zgui-bench/src/bin/resize-slope/verdict.rs` in a commit that changes \
                 nothing else and says why.",
                TOLERANCE * 100.0,
                if ratio > RECORDED {
                    "A configure is doing work the document change is not: look for work \
                     proportional to the document being done more than once per configure."
                } else {
                    "A configure is doing less than the document change: either configures are \
                     being coalesced away where they were not before, or the content change got \
                     more expensive and the baseline is what moved."
                }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RECORDED, TOLERANCE, Verdict};

    /// Four points on a line of the given slope.
    fn line(slope: f64) -> Vec<(f64, f64)> {
        (1..=4)
            .map(|n| (f64::from(n) * 100.0, 7.0 + slope * f64::from(n) * 100.0))
            .collect()
    }

    #[test]
    fn a_run_whose_slopes_are_in_the_recorded_proportion_passes() {
        let verdict = Verdict::of(&line(RECORDED * 0.02), &line(0.02));
        assert!(verdict.passed(), "{verdict}");
    }

    #[test]
    fn a_configure_that_got_dearer_than_its_baseline_fails_and_says_which_way() {
        let verdict = Verdict::of(&line(RECORDED * 0.02 * 1.5), &line(0.02));
        assert!(!verdict.passed());
        let message = verdict.to_string();
        assert!(message.contains("REGRESSION"), "{message}");
        assert!(
            message.contains("more than once per configure"),
            "{message}"
        );
        assert!(message.contains("verdict.rs"), "{message}");
    }

    #[test]
    fn a_run_that_measured_no_line_at_all_fails_rather_than_passing_quietly() {
        // The failure a gate is most likely to acquire silently: the measurement stops working,
        // every slope is `None`, and a comparison written as "not worse than" holds.
        let verdict = Verdict::of(&[], &[]);
        assert!(!verdict.passed());
        assert!(verdict.to_string().contains("broken measurement"));
    }

    #[test]
    fn a_baseline_that_costs_nothing_per_box_fails_rather_than_dividing_by_it() {
        let verdict = Verdict::of(&line(0.02), &line(0.0));
        assert!(!verdict.passed());
        assert!(verdict.to_string().contains("no same-run baseline"));
    }

    #[test]
    fn the_band_is_the_ten_per_cent_the_gate_is_written_as() {
        assert!((TOLERANCE - 0.10).abs() < f64::EPSILON);
        let just_inside = Verdict::of(&line(RECORDED * 0.02 * 1.09), &line(0.02));
        let just_outside = Verdict::of(&line(RECORDED * 0.02 * 1.11), &line(0.02));
        assert!(just_inside.passed());
        assert!(!just_outside.passed());
    }
}
