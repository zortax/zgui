//! Where the time comes from.

mod manual;

pub use manual::VirtualClock;

use std::time::Instant;

use zgui_vocab::Timestamp;

/// The source of the present moment.
///
/// Nothing in this framework calls the system clock directly, and that is the whole reason this
/// trait exists. Timers, animations and the loop's own parking all ask *the platform* what time it
/// is, so a test backend can hand them a clock it moves by hand and a five-second animation can be
/// exercised in a microsecond, deterministically, with no sleeping and no flakiness.
///
/// A clock is monotonic: it never runs backwards, and the difference between two readings is
/// always meaningful.
pub trait Clock: Send + Sync + 'static {
    /// The present moment.
    fn now(&self) -> Instant;

    /// The moment the application started, from which every event is stamped.
    fn origin(&self) -> Instant;

    /// The present moment as an offset from the origin.
    ///
    /// This is the form an event or a frame is stamped with, and it is derived from the other two
    /// rather than tracked separately so the two readings cannot disagree.
    fn timestamp(&self) -> Timestamp {
        Timestamp::from_origin(self.now().saturating_duration_since(self.origin()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Clock, VirtualClock};
    use zgui_vocab::Timestamp;

    #[test]
    fn a_clock_that_never_moves_reports_the_origin() {
        let clock = VirtualClock::new();
        assert_eq!(clock.timestamp(), Timestamp::ORIGIN);
    }

    #[test]
    fn advancing_a_virtual_clock_moves_the_timestamp_by_exactly_that_much() {
        let clock = VirtualClock::new();
        clock.advance(Duration::from_millis(700));
        assert_eq!(
            clock.timestamp(),
            Timestamp::from_origin(Duration::from_millis(700))
        );
        clock.advance(Duration::from_millis(1));
        assert_eq!(clock.timestamp().since_origin(), Duration::from_millis(701));
    }

    #[test]
    fn a_clock_is_usable_behind_a_trait_object() {
        let clock: Box<dyn Clock> = Box::new(VirtualClock::new());
        assert!(clock.now() >= clock.origin());
    }
}
