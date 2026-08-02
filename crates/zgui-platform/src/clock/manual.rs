//! A clock that only moves when it is told to.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::clock::Clock;

/// A monotonic [`Clock`] under the caller's control.
///
/// Nothing above the platform contract reads the system clock: timers, animations and the frame
/// loop's own parking all ask the platform what time it is. That is what makes a
/// seven-hundred-millisecond delay exercisable in a microsecond, with no sleeping and nothing to
/// flake.
///
/// This is the one implementation of that idea. A backend with no windowing system behind it
/// drives its surfaces from one, and so does a test harness that runs frames by hand; both hold
/// this type rather than writing a second one, because two clocks that were meant to behave
/// identically are two clocks that can stop doing so.
///
/// ```
/// use std::time::Duration;
/// use zgui_platform::{Clock, VirtualClock};
///
/// let clock = VirtualClock::new();
/// assert_eq!(clock.now(), clock.origin());
///
/// clock.advance(Duration::from_millis(700));
/// assert_eq!(clock.timestamp().since_origin(), Duration::from_millis(700));
/// ```
#[derive(Debug)]
pub struct VirtualClock {
    /// When the application started.
    origin: Instant,
    /// The present moment.
    now: Mutex<Instant>,
}

impl VirtualClock {
    /// A clock parked at its own origin.
    pub fn new() -> Self {
        let origin = Instant::now();
        Self {
            origin,
            now: Mutex::new(origin),
        }
    }

    /// Moves the present moment forward.
    ///
    /// Forward only: a clock never runs backwards, and one that could be rewound would be able to
    /// produce a state no real loop can reach.
    pub fn advance(&self, by: Duration) {
        *self.now.lock().expect("the clock is not poisoned") += by;
    }
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for VirtualClock {
    fn now(&self) -> Instant {
        *self.now.lock().expect("the clock is not poisoned")
    }

    fn origin(&self) -> Instant {
        self.origin
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::VirtualClock;
    use crate::clock::Clock;

    #[test]
    fn a_clock_that_is_not_moved_does_not_move() {
        let clock = VirtualClock::new();
        let first = clock.now();
        assert_eq!(clock.now(), first);
    }

    #[test]
    fn advancing_moves_it_by_exactly_that_much() {
        let clock = VirtualClock::new();
        clock.advance(Duration::from_millis(699));
        clock.advance(Duration::from_millis(1));
        assert_eq!(clock.timestamp().since_origin(), Duration::from_millis(700));
    }
}
