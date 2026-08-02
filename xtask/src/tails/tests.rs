//! The planted violation, and the run that has none.

use crate::tails::check::violations;

/// A run in which every duration carries its distribution and every scenario its pacing.
const WHOLE: &str = "\
MEASURE\tscroll\tscroll.translation\tus\t242.0000\t350.0000\tok\tmeasured\t1000.0000\tmet\t\
p50=242.0000;p95=910.0000;p99=1804.0000;max=23600.0000;n=540
MEASURE\tscroll\tscroll.translation.restyles\telems\t0.0000\t0.0000\tok\tnone\t0.0000\tmet\t-
PACE\tscroll\t8333.0000\t7\t540
";

#[test]
fn tails_gate_rejects_a_median_only_measurement() {
    // The planted violation: the same run with the distribution taken off the one duration in it.
    let median_only = WHOLE.replace(
        "p50=242.0000;p95=910.0000;p99=1804.0000;max=23600.0000;n=540",
        "-",
    );
    let found = violations(&median_only);
    assert_eq!(
        found.len(),
        5,
        "four quantiles and a population size are missing, and each is named: {found:?}"
    );
    assert!(
        found
            .iter()
            .all(|found| found.subject == "scroll.scroll.translation")
    );

    // Both halves. The unplanted run passes, or the assertion above is about a gate that fails
    // everything.
    assert_eq!(violations(WHOLE), Vec::new());
}

#[test]
fn tails_gate_rejects_a_scenario_that_never_published_its_late_frames() {
    let unpaced: String = WHOLE
        .lines()
        .filter(|line| !line.starts_with("PACE\t"))
        .map(|line| format!("{line}\n"))
        .collect();
    let found = violations(&unpaced);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].subject, "scroll");
    assert!(found[0].reason.contains("late-frame count"));
}

#[test]
fn tails_gate_rejects_a_late_frame_count_against_no_refresh() {
    let unrefreshed = WHOLE.replace("PACE\tscroll\t8333.0000", "PACE\tscroll\t0.0000");
    let found = violations(&unrefreshed);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].reason.contains("names no refresh"));
}

#[test]
fn a_count_is_not_asked_for_a_tail() {
    // The complement of the first test, and the reason the rule is by unit rather than by name: a
    // count is one number about a design and has no distribution to publish.
    let counts_only: String = WHOLE
        .lines()
        .filter(|line| !line.contains("\tus\t"))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(violations(&counts_only), Vec::new());
}
