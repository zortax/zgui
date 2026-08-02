//! The stamp every trace line carries.

use std::time::{SystemTime, UNIX_EPOCH};

/// Microseconds on the wall clock.
///
/// The wall clock rather than a monotonic origin because the lines this stamps are interleaved with
/// lines written by the runtime, which is a different crate with a different first call — and two
/// relative clocks cannot be subtracted from one another. Nothing here measures a duration long
/// enough for a clock adjustment to matter.
pub(crate) fn micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
}
