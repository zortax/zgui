//! Comparing two samples of the live counts, which is the whole of the check.

use zgui_profile::{Counter, Counters};

/// One live count that was larger late in a run than it was early in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Grew {
    /// Which count.
    pub(crate) counter: Counter,
    /// What it read at the early sample.
    pub(crate) early: u64,
    /// What it read at the late sample.
    pub(crate) late: u64,
}

impl Grew {
    /// How much it gained.
    pub(crate) fn by(self) -> u64 {
        self.late.saturating_sub(self.early)
    }
}

/// Every live count that is larger in `late` than in `early`.
///
/// The band is zero and there is no tolerance in it, because the quantity is a count and a count is
/// a property of the design: a document driven through a thousand identical ticks holds exactly
/// what it held after ten of them, or something is being interned and never given back. A shrinking
/// count is not a violation — a cache that reached its working set and then let some of it go is
/// working — so the comparison is one-sided.
pub(crate) fn grown(early: &Counters, late: &Counters) -> Vec<Grew> {
    Counter::live()
        .filter_map(|counter| {
            let (before, after) = (early.get(counter), late.get(counter));
            (after > before).then_some(Grew {
                counter,
                early: before,
                late: after,
            })
        })
        .collect()
}
