//! Reading and writing the frame's counters.

#[cfg(not(any(feature = "counters", debug_assertions)))]
mod inert;
#[cfg(any(feature = "counters", debug_assertions))]
mod live;

#[cfg(not(any(feature = "counters", debug_assertions)))]
use crate::counter::store::inert as backing;
#[cfg(any(feature = "counters", debug_assertions))]
use crate::counter::store::live as backing;

use crate::counter::table::{Counter, Counters};

/// Whether the counters are recording.
///
/// They are compiled in whenever debug assertions are on, and whenever the `counters` feature is
/// enabled. When this is `false` every function below is an empty inlined body and the storage
/// behind them does not exist, so a call site costs nothing at all — which is what lets
/// instrumentation be written wherever it is useful rather than only where it is affordable.
///
/// A test harness that needs to read counters from an optimised build enables the feature. It is a
/// feature rather than a debug-only compilation because a counter nobody outside a debug build can
/// read is a counter that quietly stops being maintained.
///
/// A measurement that must work in either kind of build asks first:
///
/// ```
/// use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
///
/// counter::bump(Counter::Wakes);
/// if COUNTERS_ENABLED {
///     assert_eq!(counter::get(Counter::Wakes), 1);
/// }
/// # counter::reset();
/// ```
pub const COUNTERS_ENABLED: bool = cfg!(any(feature = "counters", debug_assertions));

/// Adds one to `counter`.
#[inline]
pub fn bump(counter: Counter) {
    backing::add(counter, 1);
}

/// Adds `amount` to `counter`.
#[inline]
pub fn add(counter: Counter, amount: u64) {
    backing::add(counter, amount);
}

/// Replaces `counter`'s value with `amount`.
///
/// For a counter in [`Group::Live`](crate::counter::Group::Live), and for nothing else. A total
/// that was assigned rather than accumulated would lose every increment the frame made before the
/// assignment; a gauge that was accumulated rather than assigned would report the sum of every
/// length it has ever had, which is a number about nothing.
#[inline]
pub fn set(counter: Counter, amount: u64) {
    backing::set(counter, amount);
}

/// Reads one counter.
///
/// Reads zero when [`COUNTERS_ENABLED`] is `false`.
#[inline]
pub fn get(counter: Counter) -> u64 {
    backing::get(counter)
}

/// Reads every counter at once.
///
/// This is how a test reads a frame's cost: reset before the frame, snapshot after it, and assert
/// on the fields.
#[inline]
pub fn snapshot() -> Counters {
    backing::snapshot()
}

/// Sets every counter back to zero.
///
/// Counters accumulate until this is called; nothing resets them per frame on its own, so a
/// measurement decides for itself what interval it is measuring.
#[inline]
pub fn reset() {
    backing::reset();
}
