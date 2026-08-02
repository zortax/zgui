//! The glyph atlas, and the glyph placements remembered beside it.

use zgui_paint::ContentCache;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};

/// The window's rasterised content, as the budget sees it.
///
/// One cache and not two, although two things are held: the tiles in the atlas and, beside them,
/// what each glyph key rasterised to. They are registered together because they are freed together
/// and are meaningless apart — a remembered placement whose tile has gone names a rectangle
/// something else now occupies, which is the failure mode the whole eviction work exists to prevent.
///
/// No device is reachable from here, and none is needed. Freeing a tile returns its rectangle to an
/// allocator and records that a texture is to be destroyed; the destruction itself leaves with the
/// window's next flush, along with everything else the cache decided while nothing had a device.
pub struct GlyphAtlasBudget<'a> {
    /// The tiles and the placements.
    content: &'a mut ContentCache,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> GlyphAtlasBudget<'a> {
    /// The adapter over one window's content cache.
    pub fn new(content: &'a mut ContentCache, tracked: &'a mut Tracked) -> Self {
        Self { content, tracked }
    }
}

impl Budgeted for GlyphAtlasBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::GlyphAtlas
    }

    /// What the window installed on the atlas itself.
    ///
    /// Read from the atlas rather than restated here, so that the level a budget enforces and the
    /// level the atlas frees back down to cannot drift apart. A window that wants a different one
    /// sets it through
    /// [`ContentCache::set_soft_bytes`](zgui_paint::ContentCache::set_soft_bytes).
    fn limit(&self) -> Option<u64> {
        self.content.atlas().limits().soft_bytes
    }

    fn report(&self) -> CacheReport {
        let atlas = self.content.report();
        CacheReport {
            resident: self.content.resident_bytes(),
            // The tiles a live record holds. A replayed range draws from these and looks none of
            // them up, so this — not the frame's lookups — is what says they are still on screen.
            pinned: atlas.referenced_bytes,
            last_used: self.tracked.last_used(),
            // A rasteriser over a face already loaded, plus an upload.
            rebuild_cost: rebuild::RECOMPUTED,
            speculative: 0,
            unit: CacheUnit::Bytes,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        let held = self.content.report().referenced_tiles > 0;
        self.tracked.note(epoch, self.content.atlas().hits(), held);
    }

    /// Frees cold generations until the atlas is back under the level it states.
    ///
    /// `units` is honoured by construction rather than by arithmetic: the manager computes it by
    /// subtracting [`limit`](GlyphAtlasBudget::limit) from what is resident, and this frees back
    /// down to that same level, so what comes back is the excess or everything the atlas is allowed
    /// to give. It can be less — a frame whose own working set is larger than the level stays over
    /// it rather than dropping what it is drawing.
    fn evict(&mut self, _units: u64, _epoch: SceneEpoch) -> u64 {
        let before = self.content.resident_bytes();
        self.content.enforce_soft_limit();
        before.saturating_sub(self.content.resident_bytes())
    }

    /// Drops every tile, destroys every texture, and forgets every remembered placement.
    ///
    /// The attached images survive, because their texels are held on the other side of this cache
    /// and are that cache's to drop; what goes here is the upload.
    fn forget(&mut self) {
        self.content.clear();
    }
}
