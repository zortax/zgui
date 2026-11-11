//! The conversion of an [`Instant`] to a `CLOCK_MONOTONIC` timestamp, which is the clock of every
//! time value in the marks protocol.

use std::time::{Duration, Instant};

use fakeland_marks::Timestamp;

/// One instant and the monotonic reading taken with it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Anchor {
    /// The instant.
    instant: Instant,
    /// The monotonic reading at that instant.
    reading: Duration,
}

impl Anchor {
    /// Takes an anchor now. The reading is the middle of two readings around the instant, so the
    /// error is half the time of one reading.
    pub(crate) fn take() -> Self {
        let before = reading();
        let instant = Instant::now();
        let after = reading();
        Self {
            instant,
            reading: before + (after - before) / 2,
        }
    }

    /// Makes an anchor from known parts.
    #[cfg(test)]
    pub(crate) const fn new(instant: Instant, reading: Duration) -> Self {
        Self { instant, reading }
    }

    /// The monotonic timestamp of `at`.
    pub(crate) fn timestamp(&self, at: Instant) -> Timestamp {
        let reading = match at.checked_duration_since(self.instant) {
            Some(after) => self.reading + after,
            None => self.reading.saturating_sub(self.instant - at),
        };
        Timestamp(u64::try_from(reading.as_nanos()).unwrap_or(u64::MAX))
    }
}

/// The monotonic clock now.
fn reading() -> Duration {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Duration::new(now.tv_sec.unsigned_abs(), now.tv_nsec as u32)
}

#[cfg(test)]
mod tests {
    use super::{Anchor, reading};
    use std::time::{Duration, Instant};

    #[test]
    fn instants_on_both_sides_of_the_anchor_keep_their_distance() {
        let base = Instant::now();
        let anchor = Anchor::new(base + Duration::from_millis(5), Duration::from_secs(100));
        let later = anchor.timestamp(base + Duration::from_millis(8));
        let earlier = anchor.timestamp(base + Duration::from_millis(2));
        assert_eq!(later.0, 100_003_000_000);
        assert_eq!(earlier.0, 99_997_000_000);
    }

    #[test]
    fn the_anchor_agrees_with_the_monotonic_clock() {
        let anchor = Anchor::take();
        let before = reading();
        let converted = anchor.timestamp(Instant::now());
        let after = reading();
        let slack = 1_000_000;
        assert!(converted.0 + slack >= before.as_nanos() as u64);
        assert!(converted.0 <= after.as_nanos() as u64 + slack);
    }
}
