//! A portable crate that reads the standard clock, which panics in a browser.

use std::time::{Duration, Instant};

/// The time after `delay`.
pub fn after(delay: Duration) -> Instant {
    Instant::now() + delay
}
