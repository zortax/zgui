//! Where allocated content landed.

use zgui_geom::{Device, Rect};

use crate::texture::TextureId;

/// One allocation's identity inside its texture.
///
/// It is the allocator's own handle, which is what [`Atlas::remove`](crate::Atlas::remove) hands
/// back when it returns the space. It is opaque and is not a position: two tiles with adjacent
/// ids need not be adjacent in the texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId(pub u32);

/// A rectangle of a texture, holding one cached raster.
///
/// This is the whole result of a successful lookup: which texture to bind, and which part of it to
/// read. It is `Copy` and cheap, because a text-heavy frame produces one per glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasTile {
    /// Which texture holds the content.
    pub texture: TextureId,
    /// The allocation's handle within that texture.
    pub tile: TileId,
    /// The rectangle of the texture the content occupies, in texels.
    pub bounds: Rect<i32, Device>,
}
