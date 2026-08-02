//! The deferred-upload queue.

use zgui_geom::{Device, Rect};

use crate::texture::TextureId;
use crate::tile::TileId;

/// Bytes waiting to be written into a tile.
///
/// Rasterising a glyph and writing it to a texture are separated so that a frame issues its writes
/// in one batch at a point it chooses, rather than one write wherever each glyph happened to be
/// needed.
#[derive(Clone, Debug)]
pub(crate) struct PendingUpload {
    /// Which texture the bytes belong in.
    pub(crate) texture: TextureId,
    /// Which allocation they belong to.
    ///
    /// Carried so that removing an entry before its upload has flushed can drop the upload as
    /// well. Without it, the bytes would land in whatever content was allocated into the freed
    /// rectangle next.
    pub(crate) tile: TileId,
    /// The rectangle of the texture to write.
    pub(crate) bounds: Rect<i32, Device>,
    /// Tightly packed rows of texels.
    pub(crate) bytes: Vec<u8>,
}
