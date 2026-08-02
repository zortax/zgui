//! The turn-taking that makes a process-wide counter block assertable.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Held by whatever is currently reading the block as if it were its own.
static MEASURING: Mutex<()> = Mutex::new(());

/// Takes exclusive use of the counter block until the guard is dropped.
///
/// The counters are one set of process-wide atomics, and a test binary runs its tests in parallel.
/// So two measurements taken at once each include the other's work: a budget passes because the
/// other test happened to be idle, and a counter asserted to have stayed at zero stayed at zero
/// only until something else touched it. Every measurement that resets the block or reads a delta
/// across it takes this first.
///
/// A poisoning left behind by an assertion that already failed is ignored, because turning every
/// other measurement in the binary into the same panic hides whether they would have passed —
/// which is the one thing a suite reporting a regression most needs to say.
///
/// ```
/// use zgui_profile::{Counter, counter};
///
/// let _turn = counter::exclusive();
/// counter::reset();
/// counter::bump(Counter::Wakes);
/// if zgui_profile::COUNTERS_ENABLED {
///     assert_eq!(counter::get(Counter::Wakes), 1);
/// }
/// # counter::reset();
/// ```
///
/// # Deadlock
///
/// The guard is not reentrant. [`non_vacuity::assert_non_vacuous`](crate::counter::non_vacuity::assert_non_vacuous)
/// takes it for itself, so a caller must not be holding one when it calls that.
pub fn exclusive() -> MutexGuard<'static, ()> {
    MEASURING.lock().unwrap_or_else(PoisonError::into_inner)
}
