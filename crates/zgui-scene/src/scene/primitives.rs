//! The struct-of-arrays store, one vector per primitive kind.

use crate::group::{BackdropFilter, GroupBoundary};
use crate::prim::{
    ColorSprite, Decoration, ExternalQuad, MonoSprite, Quad, Shadow, SubpixelSprite,
};
use crate::vector::VectorItem;

/// A frame's primitives, one contiguous array per kind.
///
/// One array per kind rather than one array of an enum, because a batch of quads is then a slice
/// that copies into an instance buffer as bytes — no gather, no per-primitive branch, and no space
/// wasted padding every entry out to the largest variant.
#[derive(Clone, Debug, Default)]
pub struct Primitives {
    /// Rounded, bordered rectangles.
    pub quads: Vec<Quad>,
    /// Box shadows.
    pub shadows: Vec<Shadow>,
    /// Text decoration lines.
    pub decorations: Vec<Decoration>,
    /// Single-channel coverage sprites.
    pub mono_sprites: Vec<MonoSprite>,
    /// Three-channel coverage sprites.
    pub subpixel_sprites: Vec<SubpixelSprite>,
    /// Full-colour sprites.
    pub color_sprites: Vec<ColorSprite>,
    /// Vector content, rasterised elsewhere and composited back in.
    pub vectors: Vec<VectorItem>,
    /// Textures the renderer did not draw.
    pub externals: Vec<ExternalQuad>,
    /// Filters over the composite beneath them.
    pub backdrops: Vec<BackdropFilter>,
    /// Group start and end markers.
    pub groups: Vec<GroupBoundary>,
}

impl Primitives {
    /// Empties every array, keeping the allocations for the next frame.
    pub fn clear(&mut self) {
        self.quads.clear();
        self.shadows.clear();
        self.decorations.clear();
        self.mono_sprites.clear();
        self.subpixel_sprites.clear();
        self.color_sprites.clear();
        self.vectors.clear();
        self.externals.clear();
        self.backdrops.clear();
        self.groups.clear();
    }

    /// How many primitives there are, across every array.
    pub fn len(&self) -> usize {
        self.quads.len()
            + self.shadows.len()
            + self.decorations.len()
            + self.mono_sprites.len()
            + self.subpixel_sprites.len()
            + self.color_sprites.len()
            + self.vectors.len()
            + self.externals.len()
            + self.backdrops.len()
            + self.groups.len()
    }

    /// Whether there are no primitives at all.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
