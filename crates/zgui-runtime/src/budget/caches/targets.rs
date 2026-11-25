//! The reusable targets the renderer composes isolated content in.

use iceblink::OutputStatus;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};

/// The renderer's pooled targets, as the budget sees them.
///
/// # Which of a renderer's targets this is, and which it is not
///
/// A renderer holds two kinds and only one of them is a cache. The target a frame is composed into
/// is a live resource: it is the surface's own size, the next frame needs it, and freeing it buys
/// the length of one reallocation. The targets isolated content is composed in are the cache — how
/// many exist follows from how deeply the document nests filters and stacking contexts, they
/// outlive the frame that asked for one, and a document that has stopped isolating anything keeps
/// every one it ever needed. This is the second kind alone, and the output's snapshot
/// ([`OutputStatus::target_pool`]) is the seam it is read through.
///
/// # It states no level, and the pool is why
///
/// The pool already enforces a ceiling of its own, and enforces it in a way a second level above it
/// could not improve on: at the ceiling it lends targets at half resolution rather than growing, so
/// it does not exceed the ceiling and there is no excess for a budget to take. A level above the
/// pool's own would never fire. A level below it would be two policies pulling against each other —
/// this one freeing what the pool had decided to keep, on a document that is about to ask for it
/// again on the next frame.
///
/// Registration is for the report, which is where the pool's occupancy becomes visible beside
/// everything else the window is spending, and for [`forget`](Budgeted::forget), which is how a
/// window is put into the state a freshly opened one is in.
pub struct RenderTargetsBudget<'a> {
    /// The newest snapshot of the output whose renderer owns the pool.
    status: Option<OutputStatus>,
    /// Queues the release of every pooled target that is not lent out.
    release: &'a mut dyn FnMut(),
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> RenderTargetsBudget<'a> {
    /// The adapter over the snapshot of one window's output, whose pool `release` empties.
    pub fn new(
        status: Option<OutputStatus>,
        release: &'a mut dyn FnMut(),
        tracked: &'a mut Tracked,
    ) -> Self {
        Self {
            status,
            release,
            tracked,
        }
    }
}

impl Budgeted for RenderTargetsBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::RenderTargets
    }

    /// None, and the reason is on the type.
    fn limit(&self) -> Option<u64> {
        None
    }

    fn report(&self) -> CacheReport {
        let pool = self
            .status
            .map(|status| status.target_pool)
            .unwrap_or_default();
        CacheReport {
            resident: pool.resident,
            // What is lent out is being composited into right now. Between frames this is zero,
            // which is why a budget enforced at the end of a frame can take the whole pool.
            pinned: pool.lent,
            last_used: self.tracked.last_used(),
            // An allocation and nothing else: a pooled target is cleared before it is drawn into,
            // so none of its contents is reproduced. It is the cheapest thing in the registry to
            // get back, which is why among equally cold caches it is asked first.
            rebuild_cost: rebuild::ARITHMETIC,
            speculative: 0,
            unit: CacheUnit::Bytes,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        let leases = self.status.map_or(0, |status| status.target_pool.leases);
        self.tracked.note(epoch, leases, false);
    }

    /// Frees every target that is not lent out, once the driver applies the release.
    ///
    /// Reached only by a caller enforcing a level this cache does not state, so in a window driven
    /// by the frame loop it is not reached at all — [`forget`](Budgeted::forget) is. The release is
    /// queued, so what it frees shows in the next snapshot and this answers what is resident now.
    fn evict(&mut self, _units: u64, _epoch: SceneEpoch) -> u64 {
        (self.release)();
        self.status.map_or(0, |status| {
            status
                .target_pool
                .resident
                .saturating_sub(status.target_pool.lent)
        })
    }

    fn forget(&mut self) {
        (self.release)();
    }
}
