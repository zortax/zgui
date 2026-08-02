//! One cached raster's bookkeeping.

use zgui_geom::{Device, Size};

use crate::key::AtlasKey;
use crate::tile::AtlasTile;

/// What the atlas knows about one cached raster.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Entry {
    /// The key it was cached under, so eviction can remove it from the index.
    pub(crate) key: AtlasKey,
    /// Where the content lives.
    pub(crate) tile: AtlasTile,
    /// The extent that was asked for, which is the extent of the bytes uploaded.
    pub(crate) size: Size<i32, Device>,
    /// How many callers hold this entry against eviction.
    ///
    /// Saturating in both directions: an entry whose count has run away is merely un-evictable, and
    /// releasing one that is already at zero is a no-op. Neither is silently wrong the way a
    /// wrapping decrement is.
    pub(crate) refs: u32,
    /// The frame generation the entry was last used in.
    pub(crate) generation: u64,
}

impl Entry {
    /// Whether nothing holds this entry against eviction.
    pub(crate) const fn is_unreferenced(&self) -> bool {
        self.refs == 0
    }
}
