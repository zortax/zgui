//! A portable crate that reads the clock a browser can answer.

use std::time::Duration;

/// The time after `delay`.
pub fn after(delay: Duration) -> web_time::Instant {
    web_time::Instant::now() + delay
}
