//! The report that never came.

use std::time::{Duration, Instant};

/// How long a report that a frame reached the screen may be owed before waiting for it stops.
///
/// A display says when it has shown a frame — a compositor through a frame callback, a display
/// controller through a completion event — and the next frame is paced against that report. What
/// this covers is the report that never arrives: a compositor that drops the callback, one that is
/// stopped and continued underneath the program, a surface off-screen in a way the protocol has no
/// word for, a completion lost across a mode change. Without a limit the chain ends there and the
/// display never draws again, which is the freeze a paced backend exists to remove.
///
/// The probe that follows an expiry cannot itself block, because a backend that paces frames
/// presents without waiting. So the cost of expiring too eagerly is one frame drawn a little early,
/// and the cost of expiring too late is a display that looks stopped. The floor is set accordingly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watchdog {
    /// How long to wait.
    grace: Duration,
}

impl Watchdog {
    /// The shortest wait, for an output whose interval is unknown or very fast.
    ///
    /// Long enough that no ordinary hiccup trips it, short enough that a person reading a stalled
    /// display would not have finished noticing.
    pub const FLOOR: Duration = Duration::from_millis(200);

    /// How many refresh intervals a healthy display is allowed to miss.
    const INTERVALS: u32 = 4;

    /// The watchdog for an output that refreshes every `interval`.
    pub fn for_interval(interval: Option<Duration>) -> Self {
        let scaled = interval.map_or(Duration::ZERO, |interval| interval * Self::INTERVALS);
        Self {
            grace: scaled.max(Self::FLOOR),
        }
    }

    /// When a report owed since `owed_since` stops being waited for.
    pub fn expiry(self, owed_since: Instant) -> Instant {
        owed_since + self.grace
    }

    /// Whether a report owed since `owed_since` has been waited for long enough.
    pub fn expired(self, owed_since: Instant, now: Instant) -> bool {
        now >= self.expiry(owed_since)
    }

    /// How long this waits.
    pub const fn grace(self) -> Duration {
        self.grace
    }
}

impl Default for Watchdog {
    fn default() -> Self {
        Self::for_interval(None)
    }
}

#[cfg(test)]
mod tests {
    use super::Watchdog;
    use std::time::{Duration, Instant};

    #[test]
    fn an_unknown_interval_waits_the_floor() {
        assert_eq!(Watchdog::for_interval(None).grace(), Watchdog::FLOOR);
    }

    #[test]
    fn a_fast_output_still_waits_the_floor() {
        // Four intervals of a 240 Hz output is 16 ms, which is well inside the jitter of an
        // ordinary display under load. Expiring there would probe constantly and prove nothing.
        let fast = Watchdog::for_interval(Some(Duration::from_micros(4167)));
        assert_eq!(fast.grace(), Watchdog::FLOOR);
    }

    #[test]
    fn a_slow_output_waits_four_of_its_own_intervals() {
        let slow = Watchdog::for_interval(Some(Duration::from_millis(100)));
        assert_eq!(slow.grace(), Duration::from_millis(400));
    }

    #[test]
    fn the_wait_ends_exactly_at_its_expiry_and_not_before() {
        let watchdog = Watchdog::for_interval(None);
        let owed = Instant::now();
        assert!(!watchdog.expired(owed, owed));
        assert!(!watchdog.expired(owed, owed + Watchdog::FLOOR - Duration::from_nanos(1)));
        assert!(watchdog.expired(owed, owed + Watchdog::FLOOR));
    }
}
