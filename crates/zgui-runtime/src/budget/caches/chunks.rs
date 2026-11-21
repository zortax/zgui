//! The per-fragment paint records: each fragment's compiled painting, owned across frames.

use zgui_paint::{Painter, Tables};

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};

/// The window's paint records, as the budget sees them.
///
/// Counted in the bytes of the minted clip lists the records own. A record lives as long as its
/// fragment, so on an unvirtualised document nothing else bounds the cache.
///
/// The tables ride along because a record holds the clip chains its encoding minted, and dropping
/// one releases them.
pub struct PaintChunksBudget<'a> {
    /// The records.
    painter: &'a mut Painter,
    /// The tables the records hold entries of.
    tables: &'a mut Tables,
    /// How many chunk bytes may be held.
    limit: u64,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> PaintChunksBudget<'a> {
    /// The adapter over one window's paint records.
    pub fn new(
        painter: &'a mut Painter,
        tables: &'a mut Tables,
        limit: u64,
        tracked: &'a mut Tracked,
    ) -> Self {
        Self {
            painter,
            tables,
            limit,
            tracked,
        }
    }
}

impl Budgeted for PaintChunksBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::PaintChunks
    }

    fn limit(&self) -> Option<u64> {
        Some(self.limit)
    }

    fn report(&self) -> CacheReport {
        let cache = self.painter.cache();
        CacheReport {
            resident: cache.bytes() as u64,
            // The records selected this frame. Evicting one is legal — the next visit re-encodes
            // — but wasteful, so the working set is reported as pinned and eviction never takes
            // it.
            pinned: cache.selected_bytes() as u64,
            last_used: self.tracked.last_used(),
            // A re-encode of the fragment: lowering, emitters, and the order walk.
            rebuild_cost: rebuild::RECOMPUTED,
            speculative: 0,
            unit: CacheUnit::Bytes,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        let held = self.painter.cache().selected_bytes() > 0;
        self.tracked
            .note(epoch, self.painter.cache().selections(), held);
    }

    fn evict(&mut self, units: u64, _epoch: SceneEpoch) -> u64 {
        self.painter.evict_cold_chunks(units, self.tables)
    }

    fn forget(&mut self) {
        self.painter.clear_records(self.tables);
    }
}
