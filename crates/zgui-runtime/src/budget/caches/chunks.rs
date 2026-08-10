//! The per-fragment paint records: each fragment's compiled painting, owned across frames.

use zgui_paint::{ContentCache, Painter};
use zgui_scene::Scene;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};

/// The window's paint records, as the budget sees them.
///
/// Counted in bytes, because a record's cost is the primitives its chunk owns and those are plain
/// arrays. A record lives as long as its fragment, so on an unvirtualised document nothing else
/// bounds the cache — visits stopped being what retention follows when records started owning
/// their bytes.
///
/// The scene and the atlas ride along because a record is an owner: dropping one releases holds
/// on clip and paint table entries and on atlas tiles, and the release has to reach the caches
/// the holds are in.
pub struct PaintChunksBudget<'a> {
    /// The records.
    painter: &'a mut Painter,
    /// The tables the records hold entries of.
    scene: &'a mut Scene,
    /// The atlas the records hold tiles of.
    content: &'a mut ContentCache,
    /// How many chunk bytes may be held.
    limit: u64,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> PaintChunksBudget<'a> {
    /// The adapter over one window's paint records.
    pub fn new(
        painter: &'a mut Painter,
        scene: &'a mut Scene,
        content: &'a mut ContentCache,
        limit: u64,
        tracked: &'a mut Tracked,
    ) -> Self {
        Self {
            painter,
            scene,
            content,
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
        self.painter
            .evict_cold_chunks(units, self.scene, &self.content.tile_owner())
    }

    fn forget(&mut self) {
        self.painter
            .clear_records(self.scene, &self.content.tile_owner());
    }
}
