//! The CPU vector layers: whole drawings rasterised into the image pool of the atlas.

use zgui_paint::{ContentCache, Painter};
use zgui_scene::Scene;

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
/// Separate from the glyph atlas they share a pool with, because a layer is large and its level
/// follows the surface: a few zoomed drawings can hold more texels than every glyph on the page.
/// The atlas reports its figures without them, so the registry counts them once.
///
/// A paint record holds the layers it draws for as long as its fragment lives, so the layers of
/// drawings scrolled far away are held too. Eviction first removes the layers nothing holds, and
/// then drops the coldest records that hold layers, which releases those layers as well. A dropped
/// record is a clean miss: the next frame that reaches its fragment encodes it again.
pub struct VectorLayersBudget<'a> {
    /// The records that hold layers.
    painter: &'a mut Painter,
    /// The tables the records hold entries of.
    scene: &'a mut Scene,
    /// The layers and the atlas they are in.
    content: &'a mut ContentCache,
    /// How many layer bytes may be held.
    limit: u64,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> VectorLayersBudget<'a> {
    /// The adapter over one window's layers.
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
            // The layers this frame drew. A layer a record holds can go with the record.
            pinned: self.content.layer_pinned_bytes(),
            last_used: self.tracked.last_used(),
            // A CPU raster of a drawing already in memory, and an upload.
            rebuild_cost: rebuild::RECOMPUTED,
            speculative: 0,
            unit: CacheUnit::Bytes,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        let held = self.content.layer_held_bytes() > 0;
        self.tracked.note(epoch, self.content.layer_hits(), held);
    }

    fn evict(&mut self, units: u64, _epoch: SceneEpoch) -> u64 {
        let mut freed = self.content.evict_layers(units);
        if freed >= units {
            return freed;
        }
        let tiles = self.content.layer_tiles();
        let dropped = self.painter.evict_cold_holding(
            units - freed,
            &|key| tiles.get(&key).copied(),
            self.scene,
            &self.content.tile_owner(),
        );
        if dropped > 0 {
            freed += self.content.evict_layers(units - freed);
        }
        freed
    }

    /// Removes every layer nothing holds.
    fn forget(&mut self) {
        self.content.evict_layers(u64::MAX);
    }
}
