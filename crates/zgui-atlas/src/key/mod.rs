//! What a caller caches by.

use crate::texture::TextureKind;

/// An opaque cache key, paired with the pool its content belongs in.
///
/// The handle is a `u64` the caller derives however it likes — typically a hash of everything that
/// changes the pixels: for a glyph, the font, the glyph id, the size, the subpixel phase and the
/// hinting mode; for an image, the source identity and the decode size. The atlas compares handles
/// and never interprets them, so what counts as cacheable content is entirely the caller's
/// vocabulary and not this crate's.
///
/// Two keys with the same handle and different kinds are different keys, because they would land
/// in different pools with different formats.
///
/// ```
/// use zgui_atlas::{AtlasKey, TextureKind};
///
/// let glyph = AtlasKey::new(0x1234, TextureKind::Mono);
/// assert_eq!(glyph, AtlasKey::new(0x1234, TextureKind::Mono));
/// assert_ne!(glyph, AtlasKey::new(0x1234, TextureKind::Color));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AtlasKey {
    /// Which pool the content belongs in.
    kind: TextureKind,
    /// The caller's identity for the content.
    handle: u64,
}

impl AtlasKey {
    /// A key for `handle`'s content in `kind`'s pool.
    pub const fn new(handle: u64, kind: TextureKind) -> Self {
        Self { kind, handle }
    }

    /// The caller's identity for the content.
    pub const fn handle(self) -> u64 {
        self.handle
    }

    /// Which pool the content belongs in.
    pub const fn kind(self) -> TextureKind {
        self.kind
    }
}
