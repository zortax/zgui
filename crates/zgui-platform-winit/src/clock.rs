//! Where the time comes from when it comes from the machine.

use std::time::Instant;

use zgui_platform::Clock;

/// The machine's own monotonic clock.
///
/// Nothing above the platform contract reads the system clock directly, so this is the one place
/// in a windowed program that does. It is a plain reading with an origin taken once, and the
/// origin is what every event and every frame is stamped against — kept here rather than
/// recomputed so that two readings taken in the same frame cannot disagree about when the
/// application started.
///
/// ```
/// use zgui_platform::Clock;
/// use zgui_platform_winit::SystemClock;
///
/// let clock = SystemClock::new();
/// assert!(clock.now() >= clock.origin());
/// ```
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
}
