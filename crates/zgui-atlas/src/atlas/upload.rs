//! The deferred-upload queue.

use std::sync::Arc;

use zgui_geom::{Device, Rect};

use crate::texture::TextureId;
use crate::tile::TileId;

/// The texels a pending upload owns, either exclusively or shared with their producer.
///
/// A raster built for the atlas — a glyph, a mask — arrives owned and is dropped once written. A
/// decoded image is held by its cache for as long as it is shown, and cloning it for the queue
/// would double a multi-megabyte allocation for the frames the write waits; the queue takes shared
/// ownership of that buffer instead and drops its reference after the write.
#[derive(Clone, Debug)]
pub enum UploadBytes {
    /// Texels the queue alone owns.
    Owned(Vec<u8>),
    /// Texels shared with whoever produced them.
    Shared(Arc<Vec<u8>>),
}

impl UploadBytes {
    /// The texels, however they are owned.
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(bytes) => bytes,
            Self::Shared(bytes) => bytes,
        }
    }

    /// How many bytes there are.
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Whether there are no bytes.
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

impl From<Vec<u8>> for UploadBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Owned(bytes)
    }
}

impl From<Arc<Vec<u8>>> for UploadBytes {
    fn from(bytes: Arc<Vec<u8>>) -> Self {
        Self::Shared(bytes)
    }
}

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
    /// Which level of detail to write; zero for everything except a mipped standalone tile.
    pub(crate) mip: u32,
    /// The rectangle of the texture to write, in that level's coordinate space.
    pub(crate) bounds: Rect<i32, Device>,
    /// Tightly packed rows of texels.
    pub(crate) bytes: UploadBytes,
}
