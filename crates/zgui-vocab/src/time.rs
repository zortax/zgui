//! The instant an event or a frame happened at.

use core::fmt::{self, Debug};
use core::ops::{Add, Sub};
use core::time::Duration;

/// A monotonic instant, measured from the moment the application started.
///
/// Wall-clock time is the wrong instrument for anything the frame loop does: it jumps, it can run
/// backwards, and two readings of it cannot be subtracted safely. Every time an event, an
/// animation or a timer is stamped with here is instead an offset from one fixed origin, so
/// differences are always meaningful and never negative by surprise.
///
/// A [`Timestamp`] is deliberately *not* a wall-clock time and must never be displayed as one.
///
/// ```
/// use core::time::Duration;
/// use zgui_vocab::Timestamp;
///
/// let start = Timestamp::ORIGIN;
/// let later = start + Duration::from_millis(16);
/// assert_eq!(later - start, Duration::from_millis(16));
/// assert!(later > start);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timestamp(Duration);

impl Timestamp {
    /// The moment the application started, from which every other timestamp is measured.
    pub const ORIGIN: Self = Self(Duration::ZERO);

    /// The timestamp `elapsed` after the origin.
    pub const fn from_origin(elapsed: Duration) -> Self {
        Self(elapsed)
    }

    /// How long after the origin this timestamp is.
    pub const fn since_origin(self) -> Duration {
        self.0
    }

    /// How long after `earlier` this timestamp is, saturating at zero when it is not later.
    ///
    /// Saturation rather than a panic is the right behaviour for a value that is routinely
    /// compared across sources — a platform event stamped by the compositor against a frame
    /// stamped by the loop — where a few microseconds of disagreement is ordinary.
    pub const fn saturating_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}

impl Add<Duration> for Timestamp {
    type Output = Self;

    fn add(self, duration: Duration) -> Self {
        Self(self.0 + duration)
    }
}

impl Sub for Timestamp {
    type Output = Duration;

    fn sub(self, earlier: Self) -> Duration {
        self.0 - earlier.0
    }
}

impl Debug for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Timestamp(+{:?})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Timestamp;
    use core::time::Duration;

    #[test]
    fn differences_saturate_rather_than_underflow() {
        let early = Timestamp::from_origin(Duration::from_millis(1));
        let late = Timestamp::from_origin(Duration::from_millis(4));
        assert_eq!(late.saturating_since(early), Duration::from_millis(3));
        assert_eq!(early.saturating_since(late), Duration::ZERO);
    }

    #[test]
    fn the_origin_is_the_zero_offset() {
        assert_eq!(Timestamp::ORIGIN.since_origin(), Duration::ZERO);
        assert_eq!(Timestamp::default(), Timestamp::ORIGIN);
    }
}
