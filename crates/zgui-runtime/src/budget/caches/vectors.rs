//! The placed outlines this window's drawings have been fitted into their boxes as.

use zgui_paint::VectorCache;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};

/// The window's placed drawings, as the budget sees them.
///
/// Counted in drawings rather than in bytes for the same reason shaping is: what a placed drawing
/// costs is inside the curve allocations, and those are shared with the display list — a byte
/// figure would either miss them or count them twice, and neither is a number to hold a window to.
///
/// # What the level is for
///
/// It is not what usually bounds this cache. Every frame drops the entry for every node the
/// document no longer holds, so what remains follows the live tree and a document that scrolls
/// through a thousand icons keeps none of the ones that have gone. What the level bounds is the case
/// that does not reach: a document that genuinely holds tens of thousands of live drawing nodes and
/// has painted all of them.
///
/// Reaching it is cheap by the standards of this registry. A placed drawing is produced again by
/// parsing the notation on the element and fitting it to the box, nothing measured from it is
/// invalidated, and the frame after the level fires places again exactly what it draws. The one
/// cost that is not obvious is downstream: a rasteriser keys its encoding on the identity of the
/// path allocation, so every drawing that comes back is re-encoded as well as re-placed.
pub struct VectorResourcesBudget<'a> {
    /// The placed drawings.
    vectors: &'a mut VectorCache,
    /// How many may be held.
    limit: usize,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> VectorResourcesBudget<'a> {
    /// The adapter over one window's vector cache.
    pub fn new(vectors: &'a mut VectorCache, limit: usize, tracked: &'a mut Tracked) -> Self {
        Self {
            vectors,
            limit,
            tracked,
        }
    }
}

impl Budgeted for VectorResourcesBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::VectorResources
    }

    fn limit(&self) -> Option<u64> {
        Some(self.limit as u64)
    }

    fn report(&self) -> CacheReport {
        CacheReport {
            resident: self.vectors.len() as u64,
            // Nothing. A drawing is placed from the fragment it is drawn into, so one that is on
            // the screen is produced again from the same box and the same notation.
            pinned: 0,
            last_used: self.tracked.last_used(),
            rebuild_cost: rebuild::RECOMPUTED,
            speculative: 0,
            unit: CacheUnit::Entries,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        self.tracked.note(epoch, self.vectors.hits(), false);
    }

    /// Drops every placed drawing, whatever `units` asked for.
    ///
    /// All or nothing, because the cache records no per-entry last use — the same shape as the
    /// shaping cache and for the same reason, at a small fraction of the cost.
    fn evict(&mut self, _units: u64, _epoch: SceneEpoch) -> u64 {
        self.vectors.clear() as u64
    }

    fn forget(&mut self) {
        self.vectors.clear();
    }
}
