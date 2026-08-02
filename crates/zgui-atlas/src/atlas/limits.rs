//! How large the atlas is allowed to get.

use zgui_geom::{Device, Size};

/// The bounds an [`Atlas`](crate::Atlas) allocates within.
///
/// Every field is a cap on something that would otherwise grow without one. They are constructor
/// arguments rather than constants because the real ceiling is the device's maximum texture
/// dimension, which is only known once an adapter has been chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasLimits {
    /// The extent a fresh texture is created with, when the content fits inside it.
    ///
    /// Small enough that a document using a handful of glyphs does not reserve a large texture,
    /// large enough that a text-heavy document is not paying for a new texture every few dozen
    /// glyphs.
    pub texture_size: i32,
    /// The largest extent a texture may be created with.
    ///
    /// Content larger than this cannot be cached at all, and asking for it is
    /// [`AtlasError::TooLarge`](crate::AtlasError::TooLarge) rather than a failure to find room.
    pub max_texture_size: i32,
    /// How many textures one pool may hold before allocation starts failing.
    ///
    /// Reaching it is [`AtlasError::OutOfSpace`](crate::AtlasError::OutOfSpace), which is a signal
    /// to evict and retry rather than a dead end.
    pub max_textures_per_pool: u32,
    /// The resident byte count above which cold content is freed, or `None` for an atlas that
    /// never frees anything of its own accord.
    ///
    /// Soft rather than hard: it is a level the atlas returns *below*, not a ceiling allocation is
    /// refused at. A frame is allowed to exceed it — everything one frame draws is hot, and
    /// refusing an allocation because the frame is large would drop glyphs off the screen — and
    /// [`Atlas::evict_to_soft_limit`](crate::Atlas::evict_to_soft_limit) takes the excess back out
    /// of the cold generations afterwards.
    ///
    /// `None` is the default because eviction that nothing bounds is eviction with no criterion:
    /// an atlas with no soft limit has no answer to "how much is too much", and freeing tiles
    /// without one costs a re-rasterisation for nothing.
    pub soft_bytes: Option<u64>,
}

impl Default for AtlasLimits {
    /// Textures of 1024 texels square, capped at 4096, sixteen per pool, and no soft limit.
    ///
    /// 4096 is the smallest maximum texture dimension any target device is expected to offer, so
    /// the default never depends on a capability that might be absent.
    fn default() -> Self {
        Self {
            texture_size: 1024,
            max_texture_size: 4096,
            max_textures_per_pool: 16,
            soft_bytes: None,
        }
    }
}

impl AtlasLimits {
    /// The same limits, returning below `bytes` of resident texture memory when it can.
    pub fn with_soft_bytes(self, bytes: u64) -> Self {
        Self {
            soft_bytes: Some(bytes),
            ..self
        }
    }

    /// The largest tile that can ever be cached, as a size.
    pub fn largest_tile(self) -> Size<i32, Device> {
        Size::new(self.max_texture_size, self.max_texture_size)
    }

    /// The extent a texture holding a `requested` tile must be created with.
    ///
    /// The default extent, grown to fit the tile when the tile is larger, and never beyond the
    /// maximum.
    pub fn texture_extent_for(self, requested: Size<i32, Device>) -> Size<i32, Device> {
        let fit = |wanted: i32| wanted.max(self.texture_size).min(self.max_texture_size);
        Size::new(fit(requested.width), fit(requested.height))
    }
}
