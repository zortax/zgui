//! Where each named raster currently is.

use rustc_hash::FxHashMap;
use zgui_atlas::AtlasTile;

use crate::resource::{ResourceGeneration, ResourceKey};

/// The placements a frame's names resolve through.
///
/// Only names that were *not* already placed when they were used need to be in here. A producer
/// that knows where a raster is puts the placement straight into the sprite and files nothing, so an
/// application whose content is all resolved as it is reached carries an empty registry and pays a
/// pointer for it.
#[derive(Debug, Default)]
pub struct ResourceRegistry {
    /// Where each name's texels are.
    placed: FxHashMap<ResourceKey, AtlasTile>,
    /// Which lifetime of the cache the names in here belong to.
    generation: ResourceGeneration,
}

impl ResourceRegistry {
    /// An empty registry in the first generation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Which lifetime of the cache the names in here belong to.
    pub fn generation(&self) -> ResourceGeneration {
        self.generation
    }

    /// Discards every placement and moves to the next generation.
    ///
    /// This is the device-loss step, and the generation is what makes it safe: a key handed out
    /// before it is not the same key as one handed out after, so a sprite still carrying the old
    /// one cannot be resolved to whatever has since taken that content's place.
    pub fn discard(&mut self) {
        self.placed.clear();
        self.generation = self.generation.next();
    }

    /// How many names have a placement.
    pub fn len(&self) -> usize {
        self.placed.len()
    }

    /// Whether nothing has a placement.
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }

    /// Records where `key`'s texels are.
    pub fn place(&mut self, key: ResourceKey, tile: AtlasTile) {
        self.placed.insert(key, tile);
    }

    /// Where `key`'s texels are, or `None` when nothing here knows.
    ///
    /// A key from another generation is not resolved and is not an error: the pixels it named are
    /// gone, and the frame that pushed it has to be told rather than handed a plausible rectangle.
    pub fn tile(&self, key: ResourceKey) -> Option<AtlasTile> {
        if key.generation() != self.generation {
            return None;
        }
        self.placed.get(&key).copied()
    }
}
