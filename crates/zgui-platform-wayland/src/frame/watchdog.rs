//! The callback that never came.

use std::time::{Duration, Instant};

/// How long a frame callback may be owed before the surface stops waiting for it.
///
/// A compositor that stops drawing a surface says so, and a surface told that stops asking. What
/// this covers is the case where it does not say so: a compositor that drops the callback, one
/// that is stopped and continued underneath us, one whose surface is off-screen in a way the
/// protocol has no word for. Without a limit the frame chain ends there and the window never draws
/// again — the freeze this backend exists to remove.
///
/// The probe that follows an expiry cannot itself block, because presentation on this backend
/// never waits. So the cost of expiring too eagerly is one frame drawn a little early, and the
/// cost of expiring too late is a window that looks stopped. The floor is set accordingly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watchdog {
    /// How long to wait.
    grace: Duration,
}

impl Watchdog {
    /// The shortest wait, for an output whose interval is unknown or very fast.
    ///
    /// Long enough that no ordinary compositor hiccup trips it, short enough that a person reading
    /// a stalled window would not have finished noticing.
    pub const FLOOR: Duration = Duration::from_millis(200);

    /// How many refresh intervals a healthy compositor is allowed to miss.
    const INTERVALS: u32 = 4;

    /// The watchdog for an output that refreshes every `interval`.
    pub fn for_interval(interval: Option<Duration>) -> Self {
        let scaled = interval.map_or(Duration::ZERO, |interval| interval * Self::INTERVALS);
        Self {
            grace: scaled.max(Self::FLOOR),
        }
    }

    /// When a callback owed since `owed_since` stops being waited for.
    pub fn expiry(self, owed_since: Instant) -> Instant {
        owed_since + self.grace
    }

    /// Whether a callback owed since `owed_since` has been waited for long enough.
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
        // ordinary compositor under load. Expiring there would probe constantly and prove nothing.
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
