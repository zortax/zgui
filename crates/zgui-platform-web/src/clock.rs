//! The page's monotonic clock.

use std::time::Duration;

use web_time::Instant;
use zgui_platform::Clock;
use zgui_vocab::Timestamp;

/// The clock of one page: `performance.now()`, with an origin taken when the application started.
#[derive(Clone, Copy, Debug)]
pub struct PageClock {
    /// The moment the clock was made.
    origin: Instant,
    /// The same moment in the page's own milliseconds, which is what an event is stamped in.
    origin_ms: f64,
}

impl PageClock {
    /// A clock whose origin is now.
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
            origin_ms: now_ms(),
        }
    }

    /// The timestamp of a moment the browser stamped in milliseconds, such as an event's time or
    /// an animation frame's.
    pub fn stamp(&self, page_ms: f64) -> Timestamp {
        let elapsed = (page_ms - self.origin_ms).max(0.0);
        Timestamp::from_origin(Duration::from_secs_f64(elapsed / 1000.0))
    }
}

impl Default for PageClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for PageClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn origin(&self) -> Instant {
        self.origin
    }
}

/// The page's clock in milliseconds.
pub(crate) fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}
