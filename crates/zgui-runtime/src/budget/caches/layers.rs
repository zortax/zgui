//! The CPU vector layers: whole drawings rasterised into images.

use zgui_paint::ContentCache;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};

/// The smallest level the layers are held to.
pub const MIN_LAYER_BYTES: u64 = 16 * 1024 * 1024;

/// The largest level the layers are held to.
pub const MAX_LAYER_BYTES: u64 = 128 * 1024 * 1024;

/// The level for a surface of `width` by `height` device pixels: four surfaces of RGBA8, within
/// [`MIN_LAYER_BYTES`] and [`MAX_LAYER_BYTES`].
pub fn layer_limit(width: u32, height: u32) -> u64 {
    (4 * u64::from(width) * u64::from(height) * 4).clamp(MIN_LAYER_BYTES, MAX_LAYER_BYTES)
}

/// The window's CPU vector layers, as the budget sees them.
///
/// A layer is large and its level follows the surface: a few zoomed drawings can hold more texels
/// than every glyph on the page.
///
/// Eviction removes the layers the frame did not draw, coldest first. The compositor keeps the
/// image of a removed layer while a chunk draws it, and the next frame that encodes its fragment
/// rasterises it again.
pub struct VectorLayersBudget<'a> {
    /// The layers.
    content: &'a mut ContentCache,
    /// How many layer bytes may be held.
    limit: u64,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> VectorLayersBudget<'a> {
    /// The adapter over one window's layers.
    pub fn new(content: &'a mut ContentCache, limit: u64, tracked: &'a mut Tracked) -> Self {
        Self {
            content,
            limit,
            tracked,
        }
    }
}

impl Budgeted for VectorLayersBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::VectorLayers
    }

    fn limit(&self) -> Option<u64> {
        Some(self.limit)
    }

    fn report(&self) -> CacheReport {
        CacheReport {
            resident: self.content.layer_bytes(),
            // The layers this frame drew.
            pinned: self.content.layer_pinned_bytes(),
            last_used: self.tracked.last_used(),
            // A CPU raster of a drawing already in memory, and an upload.
            rebuild_cost: rebuild::RECOMPUTED,
            speculative: 0,
            unit: CacheUnit::Bytes,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        let drawn = self.content.layer_pinned_bytes() > 0;
        self.tracked.note(epoch, self.content.layer_hits(), drawn);
    }

    fn evict(&mut self, units: u64, _epoch: SceneEpoch) -> u64 {
        self.content.evict_layers(units)
    }

    /// Removes every layer, and forgets the vector route caches beside them.
    fn forget(&mut self) {
        self.content.clear();
    }
}
