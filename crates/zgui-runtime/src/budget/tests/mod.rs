//! What the budget decides, asserted against reports written by hand.
//!
//! By hand rather than from a window on purpose. The order is a claim about what *would* happen to
//! a cache in a state this window has not been driven into — a cache holding a guess nobody has
//! used, a cache untouched for a hundred frames — and a run that never reached that state would
//! assert nothing about it. What a window is driven through is asserted in
//! `tests/budget_registry.rs`, over the registry rather than over any one cache.

mod images;
mod order;
mod tracking;

use crate::budget::report::{CacheId, CacheLine, CacheReport, CacheUnit, rebuild};
use crate::budget::{BudgetReport, SceneEpoch};

/// A line for `id`, last read at frame `used`, costing `cost` to produce again.
fn line(id: CacheId, used: u64, cost: u64) -> CacheLine {
    CacheLine {
        id,
        report: CacheReport {
            resident: 1_000,
            pinned: 0,
            last_used: at(used),
            rebuild_cost: cost,
            speculative: 0,
            unit: CacheUnit::Entries,
        },
        limit: Some(100),
    }
}

/// The frame stamp `frames` frames after the first.
fn at(frames: u64) -> SceneEpoch {
    (0..frames).fold(SceneEpoch::FIRST, |epoch, _| epoch.next())
}

/// A report in which every cache is equally warm and equally expensive.
///
/// The baseline the ordering tests perturb one field of, so that whatever moves is the field that
/// was changed and not one the fixture happened to differ in.
fn uniform() -> BudgetReport {
    BudgetReport::new(CacheId::ALL.map(|id| line(id, 10, rebuild::RECOMPUTED)))
}
