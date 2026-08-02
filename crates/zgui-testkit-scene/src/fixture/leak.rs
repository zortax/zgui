//! Rule 2, made mechanical: a leak assertion counts drops.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Counts the values created under it against the values dropped.
///
/// This is the whole instrument a leak assertion is allowed to use: the reactive engine publishes no
/// way to read its arena's occupancy, so "everything came back" has to be shown by counting drops
/// over a mount and unmount cycle. It is not the weaker instrument it looks like — the same shape
/// distinguished sixty thousand created and sixty thousand dropped from ten thousand values created
/// with no owner and never dropped at all, with no panic and no log anywhere.
///
/// ```
/// use zgui_testkit_scene::fixture::leak::DropLedger;
///
/// let ledger = DropLedger::new();
/// {
///     let _held: Vec<_> = (0..1_000).map(|_| ledger.witness()).collect();
///     assert_eq!(ledger.live(), 1_000);
/// }
/// ledger.assert_balanced();
/// ```
#[derive(Clone, Debug, Default)]
pub struct DropLedger {
    /// The counts, shared with every witness.
    counts: Arc<Counts>,
}

/// The two numbers a ledger keeps.
#[derive(Debug, Default)]
struct Counts {
    /// How many witnesses have been created.
    created: AtomicU64,
    /// How many have been dropped.
    dropped: AtomicU64,
}

impl DropLedger {
    /// A ledger with nothing counted.
    pub fn new() -> Self {
        Self::default()
    }

    /// A value whose creation and drop this ledger counts.
    pub fn witness(&self) -> Witness {
        self.counts.created.fetch_add(1, Ordering::Relaxed);
        Witness {
            counts: Arc::clone(&self.counts),
        }
    }

    /// How many witnesses have been created.
    pub fn created(&self) -> u64 {
        self.counts.created.load(Ordering::Relaxed)
    }

    /// How many have been dropped.
    pub fn dropped(&self) -> u64 {
        self.counts.dropped.load(Ordering::Relaxed)
    }

    /// How many are still alive.
    pub fn live(&self) -> u64 {
        self.created() - self.dropped()
    }

    /// Asserts that everything created has been dropped.
    ///
    /// # Panics
    ///
    /// Panics when anything is still alive — and when *nothing was ever created*, because a ledger
    /// that counted nothing balances trivially, and "zero created, zero dropped" is the exact shape
    /// a leak assertion takes when the cycle it was supposed to exercise never ran.
    pub fn assert_balanced(&self) {
        assert!(
            self.created() > 0,
            "this leak ledger counted nothing at all, so it balances without having watched \
             anything. A cycle that created no witnesses is not evidence that a cycle cleans up."
        );
        assert_eq!(
            self.live(),
            0,
            "{} of {} witnesses are still alive after the cycle",
            self.live(),
            self.created()
        );
    }
}

/// One counted value.
///
/// Holding it keeps the ledger unbalanced; dropping it balances the ledger by exactly one. It
/// carries nothing else, because what is under test is the *lifetime* and never the value.
#[derive(Debug)]
pub struct Witness {
    /// The counts it reports its drop to.
    counts: Arc<Counts>,
}

impl Drop for Witness {
    fn drop(&mut self) {
        self.counts.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::DropLedger;

    #[test]
    fn a_cycle_that_cleans_up_balances() {
        let ledger = DropLedger::new();
        for _ in 0..5 {
            let held: Vec<_> = (0..1_000).map(|_| ledger.witness()).collect();
            drop(held);
        }
        assert_eq!(ledger.created(), 5_000);
        assert_eq!(ledger.dropped(), 5_000);
        ledger.assert_balanced();
    }

    #[test]
    #[should_panic(expected = "still alive after the cycle")]
    fn a_cycle_that_leaks_is_caught() {
        // The negative control: values kept alive past the cycle are exactly what a leak looks
        // like, and the ledger has to say so rather than reporting a tidy zero.
        let ledger = DropLedger::new();
        let leaked: Vec<_> = (0..10).map(|_| ledger.witness()).collect();
        std::mem::forget(leaked);
        ledger.assert_balanced();
    }

    #[test]
    #[should_panic(expected = "counted nothing at all")]
    fn a_ledger_that_watched_nothing_is_not_evidence() {
        DropLedger::new().assert_balanced();
    }
}
