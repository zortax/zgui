//! The counter block, when the counters are compiled in.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::counter::table::{Counter, Counters};

/// One atomic per counter, and no lock anywhere.
///
/// A counter is written from whichever thread did the work — the style traversal runs across a
/// pool, and a wake can be raised from any thread at all — so the block has to be shared. Relaxed
/// atomic addition is what that costs: the counters order nothing and guard nothing, they only
/// have to end up with the right totals once the threads that wrote them have joined.
static VALUES: [AtomicU64; Counter::COUNT] = [const { AtomicU64::new(0) }; Counter::COUNT];

/// Adds `amount` to `counter`.
pub(super) fn add(counter: Counter, amount: u64) {
    VALUES[counter.index()].fetch_add(amount, Ordering::Relaxed);
}

/// Replaces `counter`'s value with `amount`.
pub(super) fn set(counter: Counter, amount: u64) {
    VALUES[counter.index()].store(amount, Ordering::Relaxed);
}

/// Reads `counter`.
pub(super) fn get(counter: Counter) -> u64 {
    VALUES[counter.index()].load(Ordering::Relaxed)
}

/// Reads every counter.
pub(super) fn snapshot() -> Counters {
    Counters::from_fn(get)
}

/// Sets every counter back to zero.
pub(super) fn reset() {
    for value in &VALUES {
        value.store(0, Ordering::Relaxed);
    }
}
