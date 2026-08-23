//! What the compositor says about when frames reached the screen.

use std::time::{Duration, Instant};

use zgui_platform::PresentationTiming;

/// The presentation feedback, accumulated into what a frame schedule is built on.
///
/// The compositor answers every committed frame with the moment it was scanned out and the
/// interval its output refreshes at, or says the frame was never shown at all. Two things come out
/// of that and nothing else in this backend can supply either.
///
/// The **interval** is the output the surface is actually on, restated on every frame. A rate read
/// once from whichever output a window was born on is wrong for the rest of that window's life the
/// moment it is dragged to another monitor, and pacing a 75 Hz output against a 120 Hz interval
/// misses three vsyncs in five.
///
/// The **phase** is when the last frame landed, which is the only way to say when the next one
/// will. Without it a frame can only be started when the previous one finished, which is a whole
/// frame of latency spent waiting for a moment that could have been predicted.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timing {
    /// When the last frame reached the screen.
    presented: Option<Instant>,
    /// The interval the compositor last reported, which is the output the surface is on.
    reported: Option<Duration>,
    /// The interval derived from the output's mode, for a compositor that reports none.
    declared: Option<Duration>,
    /// How many frames the compositor said it never showed.
    discarded: u64,
    /// How many frames have been committed whose presentation the compositor has not answered.
    ///
    /// A compositor answers every frame it composites, one way or the other. A frame it never
    /// composites is answered neither way — the feedback object is simply left — so a run of them
    /// is the compositor saying, by saying nothing, that this surface is not reaching a screen.
    awaiting: u32,
}

impl Timing {
    /// Records a frame reaching the screen at `at`, on an output refreshing every `refresh`.
    ///
    /// A refresh of zero means the output has no fixed rate — a variable-refresh display between
    /// frames — and is recorded as unknown rather than as an interval of nothing, which would make
    /// every prediction built on it fire immediately and for ever.
    pub const fn presented(&mut self, at: Instant, refresh: Duration) {
        self.presented = Some(at);
        self.awaiting = 0;
        if !refresh.is_zero() {
            self.reported = Some(refresh);
        }
    }

    /// Records a frame the compositor never showed.
    ///
    /// The phase is left alone. A discarded frame says nothing about when the display refreshed,
    /// and overwriting the phase with the moment of the discard would move the whole schedule onto
    /// a beat the output never had.
    pub const fn discarded(&mut self) {
        self.discarded += 1;
        self.awaiting = 0;
    }

    /// Records that a frame has been committed and its presentation asked about.
    pub const fn asked(&mut self) {
        self.awaiting = self.awaiting.saturating_add(1);
    }

    /// How many committed frames the compositor has not answered for.
    pub const fn awaiting(&self) -> u32 {
        self.awaiting
    }

    /// Records the interval an output's mode declares, for a compositor that reports none.
    pub const fn declares(&mut self, interval: Option<Duration>) {
        self.declared = interval;
    }

    /// How many frames were reported as never shown.
    pub const fn discards(&self) -> u64 {
        self.discarded
    }

    /// The interval to schedule against: what was measured, else what the output declares.
    pub fn interval(&self) -> Option<Duration> {
        self.reported.or(self.declared).filter(|it| !it.is_zero())
    }

    /// The refresh rate in thousandths of a hertz, as the contract asks for it.
    ///
    /// Computed in nanoseconds because the compositor reports in nanoseconds: 13.34668 ms is
    /// 74.925 Hz, and rounding the interval to whole microseconds first moves it to 74.929.
    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        let interval = self.interval()?;
        let rate = 1e12 / interval.as_nanos() as f64;
        (rate.is_finite() && rate >= 1.0).then(|| rate.round() as u32)
    }

    /// This timing as the contract reports it.
    pub fn snapshot(&self) -> PresentationTiming {
        let interval = self.interval();
        PresentationTiming {
            last_presented: self.presented,
            interval,
            next_refresh: match (self.presented, interval) {
                (Some(presented), Some(interval)) => Some(presented + interval),
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Timing;
    use std::time::{Duration, Instant};

    const HZ_75: Duration = Duration::from_nanos(13_346_680);

    #[test]
    fn a_timing_that_has_seen_nothing_answers_nothing() {
        let timing = Timing::default();
        assert_eq!(timing.interval(), None);
        assert_eq!(timing.refresh_rate_millihertz(), None);
        assert_eq!(timing.snapshot().next_refresh, None);
    }

    #[test]
    fn a_reported_interval_beats_the_one_the_output_declares() {
        // The output's mode is a nominal rate; the feedback is what the surface is actually being
        // presented at, on whichever output it is on now.
        let mut timing = Timing::default();
        timing.declares(Some(Duration::from_millis(16)));
        assert_eq!(timing.interval(), Some(Duration::from_millis(16)));
        timing.presented(Instant::now(), HZ_75);
        assert_eq!(timing.interval(), Some(HZ_75));
    }

    #[test]
    fn a_variable_refresh_frame_leaves_the_interval_unknown_rather_than_zero() {
        let mut timing = Timing::default();
        timing.presented(Instant::now(), Duration::ZERO);
        assert_eq!(timing.interval(), None);
    }

    #[test]
    fn the_rate_is_the_interval_in_thousandths_of_a_hertz() {
        let mut timing = Timing::default();
        timing.presented(Instant::now(), HZ_75);
        assert_eq!(timing.refresh_rate_millihertz(), Some(74_925));
    }

    #[test]
    fn frames_the_compositor_has_not_answered_for_are_counted_and_cleared_by_any_answer() {
        // A compositor answers every frame it composites, one way or the other. A run of frames it
        // answers neither way is the compositor saying, by saying nothing, that this surface is
        // not reaching a screen — which on several desktops is the only thing that ever says so.
        let mut timing = Timing::default();
        timing.asked();
        timing.asked();
        assert_eq!(timing.awaiting(), 2);
        timing.presented(Instant::now(), HZ_75);
        assert_eq!(timing.awaiting(), 0);

        timing.asked();
        timing.discarded();
        assert_eq!(timing.awaiting(), 0, "a discard is an answer too");
    }

    #[test]
    fn the_count_of_unanswered_frames_does_not_wrap() {
        let mut timing = Timing::default();
        for _ in 0..10 {
            timing.asked();
        }
        assert_eq!(timing.awaiting(), 10);
    }

    #[test]
    fn a_discarded_frame_is_counted_and_leaves_the_phase_alone() {
        let mut timing = Timing::default();
        let landed = Instant::now();
        timing.presented(landed, HZ_75);
        timing.discarded();
        assert_eq!(timing.discards(), 1);
        assert_eq!(timing.snapshot().last_presented, Some(landed));
        assert_eq!(timing.snapshot().next_refresh, Some(landed + HZ_75));
    }
}
