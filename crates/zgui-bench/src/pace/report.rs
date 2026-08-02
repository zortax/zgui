//! Turning a run's frame intervals into the six numbers a pacing claim is made of.

use crate::scenario::band::{Pace, Spread};

/// One second of a run, so that the beginning can be compared with the end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Second {
    /// Which second of the run this is, from zero.
    pub(crate) index: usize,
    /// The middle of the intervals inside it.
    pub(crate) p50: f64,
    /// How many intervals it held.
    pub(crate) frames: usize,
}

/// What a pacing run measured.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Report {
    /// The distribution of the frame intervals, in microseconds.
    pub(crate) intervals: Spread,
    /// How they landed against the output's refresh.
    pub(crate) pace: Pace,
    /// The first second of the run.
    pub(crate) first: Second,
    /// The last second of it.
    pub(crate) last: Second,
}

impl Report {
    /// How far above one and a half refresh intervals a frame is a missed vsync.
    ///
    /// A missed vsync is not the same event as a late interval and is counted separately: an
    /// interval is late when a frame arrived after the display had already repeated, and a vsync is
    /// missed once for every whole refresh the interval spanned. One stall of five refreshes is one
    /// late interval and four missed vsyncs, and reporting only the first makes a long stall look
    /// like a short one.
    pub(crate) fn missed_vsyncs(intervals: &[f64], refresh_us: f64) -> usize {
        intervals
            .iter()
            .map(|interval| {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "an interval is bounded by the run's own length"
                )]
                let periods = (interval / refresh_us).floor() as usize;
                periods.saturating_sub(1)
            })
            .sum()
    }

    /// The run's last second's middle divided by its first second's.
    ///
    /// The one number no gate in this project has ever had, and the one that catches a program
    /// getting slower while it runs. A median over the whole run averages the healthy beginning
    /// into the degraded end and reports something in between that was never true at any moment.
    pub(crate) fn ramp(&self) -> f64 {
        if self.first.p50 <= 0.0 {
            return 1.0;
        }
        self.last.p50 / self.first.p50
    }

    /// Whether the run ended at the pace it started at, within `tolerance` as a fraction.
    pub(crate) fn holds_its_pace(&self, tolerance: f64) -> bool {
        self.ramp() <= 1.0 + tolerance
    }

    /// The report for `intervals`, in microseconds, against an output refreshing every
    /// `refresh_us`.
    ///
    /// # Panics
    ///
    /// Panics on an empty run, because four zeroes read as a perfect one.
    pub(crate) fn of(intervals: &[f64], refresh_us: f64) -> Self {
        assert!(!intervals.is_empty(), "a pacing run measured no interval");
        let mut sorted = intervals.to_vec();
        let seconds = split(intervals);
        let last = seconds.len().saturating_sub(1);
        Self {
            intervals: Spread::of(&mut sorted),
            pace: Pace::of(intervals, refresh_us),
            first: seconds[0],
            last: seconds[last],
        }
    }
}

/// Cuts a run into whole seconds by the wall time its own intervals account for.
///
/// By elapsed time rather than by frame count, deliberately: a run that has fallen off the pace
/// produces fewer frames per second, so equal *counts* would compare the first second against
/// something much later than the last second of the run.
fn split(intervals: &[f64]) -> Vec<Second> {
    let mut seconds: Vec<Vec<f64>> = Vec::new();
    let mut elapsed_us = 0.0_f64;
    for interval in intervals {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a run is at most a few hundred seconds long"
        )]
        let index = (elapsed_us / 1e6) as usize;
        while seconds.len() <= index {
            seconds.push(Vec::new());
        }
        seconds[index].push(*interval);
        elapsed_us += interval;
    }
    seconds.retain(|second| !second.is_empty());
    seconds
        .into_iter()
        .enumerate()
        .map(|(index, mut samples)| {
            let spread = Spread::of(&mut samples);
            Second {
                index,
                p50: spread.p50,
                frames: spread.samples,
            }
        })
        .collect()
}
