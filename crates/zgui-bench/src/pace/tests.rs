//! The planted ramp, and the run that does not have one.

use crate::pace::report::Report;

/// The reference output's interval, in microseconds.
const REFRESH: f64 = 13_347.0;

/// A run of `seconds` seconds whose frame cost grows by `per_second` of itself every second.
fn ramping(seconds: usize, per_second: f64) -> Vec<f64> {
    let mut intervals = Vec::new();
    let mut elapsed = 0.0_f64;
    while elapsed < seconds as f64 * 1e6 {
        let growth = (1.0 + per_second).powf(elapsed / 1e6);
        let interval = REFRESH * growth;
        intervals.push(interval);
        elapsed += interval;
    }
    intervals
}

#[test]
fn pacing_reports_a_planted_ramp() {
    // A scenario whose frame cost grows 5 % per second, for sixty seconds.
    let report = Report::of(&ramping(60, 0.05), REFRESH);
    assert!(
        !report.holds_its_pace(0.05),
        "a run that ends 5 % per second slower than it started does not hold its pace; the ratio \
         reported was {:.3}",
        report.ramp()
    );
    assert!(
        report.ramp() > 15.0,
        "sixty seconds of compounding 5 % is more than fifteen times slower, and the report says \
         so rather than averaging it away: {:.3}",
        report.ramp()
    );
    // And the median over the whole run, which is what this instrument replaces, would have been
    // well inside anything a person would band it at.
    assert!(
        report.intervals.p50 < report.last.p50,
        "the whole-run median understates the end of the run, which is the reason this is not a \
         median"
    );
}

#[test]
fn a_run_that_holds_its_pace_is_reported_as_holding_it() {
    let flat: Vec<f64> = (0..4_500).map(|_| REFRESH).collect();
    let report = Report::of(&flat, REFRESH);
    assert!(report.holds_its_pace(0.05));
    assert_eq!(report.pace.late, 0);
    assert_eq!(Report::missed_vsyncs(&flat, REFRESH), 0);
}

#[test]
fn a_stall_is_one_late_interval_and_several_missed_vsyncs() {
    let mut run: Vec<f64> = (0..100).map(|_| REFRESH).collect();
    run.push(REFRESH * 5.0);
    let report = Report::of(&run, REFRESH);
    assert_eq!(report.pace.late, 1, "one interval was late");
    assert_eq!(
        Report::missed_vsyncs(&run, REFRESH),
        4,
        "and the display repeated four times inside it"
    );
}
