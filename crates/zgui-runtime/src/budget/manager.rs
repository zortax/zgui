//! The registry, what a registered cache has to be able to do, and the order eviction takes.

use crate::budget::epoch::SceneEpoch;
use crate::budget::limits::CacheLimits;
use crate::budget::report::{BudgetReport, CacheId, CacheLine, CacheReport};

/// A cache a window budgets.
///
/// # Why `forget` is required and not provided
///
/// Every method here is required, and [`Budgeted::forget`] is the one where that is a decision
/// rather than an accident. A provided `forget` would have to have a body, and the only body a
/// trait can supply for "drop everything" without knowing what is held is one that drops nothing —
/// so registering a cache would silently register a cache that cannot be emptied.
///
/// That matters because "empty every cache" is not a convenience here, it is a *reachable state* the
/// registry has to be able to promise. Two things need it. Eviction needs it, because a cache that
/// cannot be emptied is a cache a window under pressure can get nothing back from. And a comparison
/// between a window that has been drawing for forty steps and one built from scratch needs it,
/// because emptying the registry is what the first has to be put through to be the second — and a
/// cache that opted out would be a cache the comparison silently stopped covering, which is a check
/// that goes on passing and has stopped asking. So registering for budgeting registers for
/// `forget`, and the trait is the place that is made unavoidable rather than remembered.
///
/// The promise is checked over the whole registry rather than per cache, which is why it survives a
/// cache being added later: see `tests/budget_registry.rs`.
///
/// # What an implementation owes
///
/// * [`report`](Budgeted::report) counts in the unit the cache's own limit is stated in.
/// * [`forget`](Budgeted::forget) leaves [`CacheReport::resident`] at zero. This is asserted over
///   the whole registry, so it is not a promise anybody has to remember to check.
/// * [`evict`](Budgeted::evict) never frees anything [`CacheReport::pinned`] counts.
pub trait Budgeted {
    /// Which cache this is.
    fn id(&self) -> CacheId;

    /// The level it is expected to come back below, or `None` for a cache that states none.
    ///
    /// `None` is a position and not an omission: a cache states it when nothing it holds can be
    /// freed at all, or when something below it already bounds what it holds. Both are stated with
    /// their reason on the implementation.
    fn limit(&self) -> Option<u64>;

    /// What it is holding.
    fn report(&self) -> CacheReport;

    /// Records what the cache did during the frame `epoch` names.
    ///
    /// This is what makes [`CacheReport::last_used`] mean anything. A cache does not count frames —
    /// it has no reason to know what one is — so what it exposes instead is a running total of
    /// lookups that found something, and this is where two readings of that total are turned into
    /// "something read it this frame". Called once per frame, before any budget decision.
    fn observe(&mut self, epoch: SceneEpoch);

    /// Frees at least `units` of what it holds, or as much as it can, and reports what it freed.
    ///
    /// Less than was asked for is an ordinary answer and not a failure: everything left may be
    /// pinned, or in the frame's own working set, and a window is never made to drop what it is
    /// drawing. `epoch` is the frame the eviction is happening in, for a cache whose policy needs
    /// to know how cold something is.
    fn evict(&mut self, units: u64, epoch: SceneEpoch) -> u64;

    /// Drops everything droppable.
    ///
    /// See the note on the trait for why this is required. It is not a memory-pressure step — some
    /// caches drop things here that eviction may never take — and a caller reaching for it wants
    /// the window in the state a freshly opened one is in.
    fn forget(&mut self);
}

/// Everything a window registers for budgeting.
///
/// A visitor rather than a collection, because the caches are fields of the window and are not
/// going to be moved into a list of boxes to be budgeted: what the registry needs is to reach each
/// of them in turn, and each of them borrows a different part of the window for exactly as long as
/// it is being visited.
///
/// It is what makes the assertions over the registry cover a cache that is added later: they walk
/// this rather than naming caches, so the only thing an author has to remember is to visit the new
/// cache here — which is the same edit that registers it at all.
pub trait CacheRegistry {
    /// Calls `visit` once for every registered cache, in [`CacheId::ALL`] order.
    fn for_each(&mut self, visit: &mut dyn FnMut(&mut dyn Budgeted));
}

/// Where one cache's own history is kept.
///
/// Beside the manager rather than inside the cache: what is recorded here is a frame number, and a
/// cache that knew about frames would have to be told when one began by everything that ever holds
/// one.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tracked {
    /// The frame in which the cache was last read.
    last_used: SceneEpoch,
    /// The cache's running lookup total as of the last frame observed.
    hits: u64,
}

impl Tracked {
    /// The frame in which the cache was last read.
    pub const fn last_used(&self) -> SceneEpoch {
        self.last_used
    }

    /// Records this frame's reading of the cache's running lookup total.
    ///
    /// `in_use` is for content that is drawn without being looked up. A replayed range draws from
    /// atlas tiles and asks the atlas for none of them — that is the whole shape of the defect the
    /// record ownership fixed — so a glyph atlas serving a static label every frame reports no
    /// lookups at all while being the hottest thing in the window. What says those tiles are still
    /// on the screen is that a live record holds them.
    pub fn note(&mut self, epoch: SceneEpoch, hits: u64, in_use: bool) {
        if hits != self.hits || in_use {
            self.last_used = epoch;
        }
        self.hits = hits;
    }
}

/// A window's budget bookkeeping: which frame it is on, what each cache last did, and the levels
/// the entry-counted caches are held to.
#[derive(Clone, Copy, Debug, Default)]
pub struct Budgets {
    /// The frame the window is on.
    epoch: SceneEpoch,
    /// One slot per [`CacheId`], indexed by [`CacheId::index`].
    tracked: [Tracked; CacheId::COUNT],
    /// The levels the entry-counted caches are held to.
    limits: CacheLimits,
    /// What every cache was holding when the budget was last enforced.
    ///
    /// Kept because reading it is not free — it walks every atlas entry and every attached picture
    /// — and something that only wants to *show* the figures should not make a window do that work
    /// a second time. See [`Window::last_budget_report`](crate::Window::last_budget_report).
    last: BudgetReport,
}

impl Budgets {
    /// Bookkeeping for a window that has painted nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The frame the window is on.
    pub const fn epoch(&self) -> SceneEpoch {
        self.epoch
    }

    /// Advances to the next frame.
    pub const fn begin_frame(&mut self) {
        self.epoch = self.epoch.next();
    }

    /// One cache's history, for the adapter that is about to report it.
    pub const fn tracked(&mut self, id: CacheId) -> &mut Tracked {
        &mut self.tracked[id.index()]
    }

    /// The levels the entry-counted caches are held to.
    pub const fn limits(&self) -> CacheLimits {
        self.limits
    }

    /// Changes the levels the entry-counted caches are held to.
    ///
    /// Nothing is freed here: the new levels take effect the next time the budget is enforced.
    pub const fn set_limits(&mut self, limits: CacheLimits) {
        self.limits = limits;
    }

    /// What every cache was holding when the budget was last enforced.
    pub const fn last_report(&self) -> BudgetReport {
        self.last
    }

    /// Records the report this frame's budget step was decided from.
    pub const fn record(&mut self, report: BudgetReport) {
        self.last = report;
    }
}

/// Records what every registered cache did this frame.
pub fn observe(registry: &mut dyn CacheRegistry, epoch: SceneEpoch) {
    registry.for_each(&mut |cache| cache.observe(epoch));
}

/// What every registered cache is holding.
pub fn report(registry: &mut dyn CacheRegistry) -> BudgetReport {
    let mut lines = CacheId::ALL.map(|id| CacheLine {
        id,
        report: CacheReport::empty(crate::budget::report::CacheUnit::Entries),
        limit: None,
    });
    registry.for_each(&mut |cache| {
        let id = cache.id();
        lines[id.index()] = CacheLine {
            id,
            report: cache.report(),
            limit: cache.limit(),
        };
    });
    BudgetReport::new(lines)
}

/// Frees the excess from every cache that is over the level `report` says it is over.
///
/// Returns what came back, per cache and in that cache's own unit. Caches are taken in
/// [`eviction_order`], which matters even though each one's excess is computed independently: a
/// window under real pressure will not get all of it back, and the order decides which caches got
/// asked before it ran out.
///
/// The report is passed in rather than taken here for two reasons. The caller has one already, and
/// reading it is not quite free — it sums every attached picture. And it is what says which caches
/// are over anything at all: a frame in which nothing is over its level visits no cache, which is
/// every frame of an ordinary document.
pub fn enforce(
    registry: &mut dyn CacheRegistry,
    report: &BudgetReport,
    epoch: SceneEpoch,
) -> Vec<(CacheId, u64)> {
    let order: Vec<CacheId> = eviction_order(report)
        .into_iter()
        .filter(|id| report.line(*id).over() > 0)
        .collect();
    let mut freed = Vec::new();
    for wanted in order {
        registry.for_each(&mut |cache| {
            if cache.id() != wanted {
                return;
            }
            let Some(limit) = cache.limit() else {
                return;
            };
            let over = cache.report().resident.saturating_sub(limit);
            if over == 0 {
                return;
            }
            let back = cache.evict(over, epoch);
            if back > 0 {
                freed.push((wanted, back));
            }
        });
    }
    freed
}

/// Drops everything every registered cache holds.
pub fn forget_all(registry: &mut dyn CacheRegistry) {
    registry.for_each(&mut |cache| cache.forget());
}

/// The order caches are asked to give memory back in.
///
/// **The speculative class first, whatever its `last_used`; then coldest `last_used` first, ties
/// broken by lowest `rebuild_cost`.**
///
/// The tie-break is second and not first, and that is the whole point of it. Ordering by rebuild
/// cost alone would evict the cheapest thing to reproduce — which is a glyph that is on screen in
/// every frame, cheap precisely because it is rasterised constantly, and re-rasterised on the very
/// next frame at the cost of the eviction plus the rebuild plus the upload. Coldness is what says
/// something will not be needed again soon; cost only decides between two things that are equally
/// cold.
///
/// Speculation comes before both because a guess that has never been used is the one thing in a
/// cache known not to be needed: its `last_used` describes when it was *produced*, so a
/// freshly-produced guess would otherwise sort as the hottest thing in the window.
///
/// The final tie-break is [`CacheId`] order, so that the answer is a function of the reports and
/// two runs over the same window agree.
pub fn eviction_order(report: &BudgetReport) -> Vec<CacheId> {
    let mut order: Vec<&CacheLine> = report.lines().collect();
    order.sort_by_key(|line| {
        (
            line.report.speculative == 0,
            line.report.last_used,
            line.report.rebuild_cost,
            line.id,
        )
    });
    order.iter().map(|line| line.id).collect()
}
