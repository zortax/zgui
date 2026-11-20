//! A native backend, which keeps the standard clock.

/// The time on the compositor's clock.
pub fn now() -> std::time::Instant {
    std::time::Instant::now()
}
