//! Where the time comes from, and how the compositor's clock is placed on it.

use std::time::{Duration, Instant};

use zgui_platform::Clock;

/// The machine's own monotonic clock.
///
/// A plain reading with an origin taken once, so two readings in the same frame cannot disagree
/// about when the application started.
#[derive(Clone, Copy, Debug)]
pub struct SystemClock {
    /// The moment the application started.
    origin: Instant,
}

impl SystemClock {
    /// A clock whose origin is now.
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn origin(&self) -> Instant {
        self.origin
    }
}

/// The compositor's presentation timestamps, placed on this process's own timeline.
///
/// A presentation is reported as a reading of a clock the compositor names, which is the system's
/// monotonic clock on every implementation that reports one at all. [`Instant`] is opaque and
/// cannot be built from such a reading, so the two timelines are tied together once: both clocks
/// are read in the same breath and the offset between them is kept. Every later timestamp is
/// converted through that offset.
///
/// Anchoring once rather than per event matters. Reading the monotonic clock again on each
/// feedback would fold the dispatch latency of that event into the answer, and the answer is the
/// phase of the display — the one number the frame schedule is built on.
#[derive(Clone, Copy, Debug)]
pub struct Monotonic {
    /// The monotonic reading taken at the anchor.
    reading: Duration,
    /// The instant taken at the same moment.
    instant: Instant,
}

impl Monotonic {
    /// Anchors the two clocks against each other, now.
    pub fn anchor() -> Self {
        let instant = Instant::now();
        Self {
            reading: reading(),
            instant,
        }
    }

    /// The instant a monotonic reading of `seconds` and `nanoseconds` names.
    ///
    /// A reading before the anchor answers with the anchor, because a presentation that appears to
    /// have happened before this process could observe it is a clock the compositor does not share
    /// and a phase that must not be trusted.
    pub fn instant(&self, seconds: u64, nanoseconds: u32) -> Instant {
        let reported = Duration::new(seconds, nanoseconds);
        self.instant + reported.saturating_sub(self.reading)
    }
}

impl Default for Monotonic {
    fn default() -> Self {
        Self::anchor()
    }
}

/// The system's monotonic clock, as the compositor reports it.
fn reading() -> Duration {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Duration::new(now.tv_sec.unsigned_abs(), now.tv_nsec as u32)
}

#[cfg(test)]
mod tests {
    use super::{Monotonic, SystemClock, reading};
    use std::time::Duration;
    use zgui_platform::Clock;

    #[test]
    fn the_origin_is_taken_once_and_never_moves() {
        let clock = SystemClock::new();
        let first = clock.origin();
        let _ = clock.now();
        assert_eq!(clock.origin(), first);
    }

    #[test]
    fn a_clock_never_reads_before_its_own_origin() {
        let clock = SystemClock::new();
        assert!(clock.now() >= clock.origin());
        assert!(clock.timestamp().since_origin() >= Duration::ZERO);
    }

    #[test]
    fn the_anchor_converts_its_own_reading_back_to_its_own_instant() {
        let anchor = Monotonic::anchor();
        let taken = reading();
        let converted = anchor.instant(taken.as_secs(), taken.subsec_nanos());
        // Both readings were taken microseconds apart, so the converted instant is the anchor's
        // plus that gap. What is being asserted is that the conversion is monotonic and finite.
        assert!(converted >= anchor.instant);
        assert!(converted.duration_since(anchor.instant) < Duration::from_secs(1));
    }

    #[test]
    fn a_reading_from_before_the_anchor_is_clamped_to_it() {
        // A compositor reporting a clock this process does not share would otherwise place a
        // presentation arbitrarily far in the past, and every deadline computed from it with it.
        let anchor = Monotonic::anchor();
        assert_eq!(anchor.instant(0, 0), anchor.instant);
    }
}
