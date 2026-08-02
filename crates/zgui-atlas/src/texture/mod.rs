//! Which pool a tile lives in, what pixels it is made of, and which texture holds it.

pub mod format;
pub mod kind;

pub use crate::texture::format::TextureFormat;
pub use crate::texture::kind::TextureKind;

/// One texture of one pool.
///
/// The kind travels with the index because the pools are separate: texture 0 of the monochrome
/// pool and texture 0 of the colour pool are different textures with different formats, and a
/// bare index could not tell them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId {
    /// Which pool.
    pub kind: TextureKind,
    /// Position within that pool.
    pub index: u32,
}

impl TextureId {
    /// The texture at `index` of `kind`'s pool.
    pub const fn new(kind: TextureKind, index: u32) -> Self {
        Self { kind, index }
    }

    /// The pixel format this texture holds, which follows from its pool.
    pub const fn format(self) -> TextureFormat {
        self.kind.format()
    }
}
