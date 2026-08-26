//! Where the time comes from on a machine with no display server.

use std::time::Instant;

use zgui_platform::Clock;

/// The machine's own monotonic clock.
///
/// Nothing above the platform contract reads the system clock directly, so this is the one place in
/// this backend that does. A reading is [`Instant::now`], and the origin is the moment the
/// application started. Every event and every frame is stamped against that origin.
///
/// The kernel reports a page flip with the moment of the vertical blank on the monotonic clock as
/// well, so a frame's deadline and the blank it reached are quantities of the same kind.
///
/// ```
/// use std::time::Duration;
///
/// use zgui_platform::Clock;
/// use zgui_platform_drm::SystemClock;
///
/// let clock = SystemClock::new();
/// let origin = clock.origin();
///
/// assert_eq!(clock.origin(), origin, "the origin is taken once, at start-up");
/// assert!(
///     clock.timestamp().since_origin() > Duration::ZERO,
///     "so a stamp measures from start-up rather than from the reading beside it"
/// );
/// ```
#[derive(Clone, Copy, Debug)]
pub struct SystemClock {
    /// The moment the application started.
    ///
    /// Held rather than read again on each call. Two readings taken in one frame would otherwise
    /// disagree about when the application started, and every timestamp is a difference from this.
    origin: Instant,
}

impl SystemClock {
    /// Returns a clock whose origin is now.
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

#[cfg(test)]
mod tests {
    use super::SystemClock;
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
        assert!(clock.timestamp().since_origin() >= std::time::Duration::ZERO);
    }

    #[test]
    fn a_clock_never_runs_backwards() {
        let clock = SystemClock::new();
        let mut last = clock.now();
        for _ in 0..1_000 {
            let reading = clock.now();
            assert!(reading >= last, "a monotonic clock never reads earlier");
            last = reading;
        }
    }
}
