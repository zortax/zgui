//! The two pixel formats an atlas texture is ever created with.

/// The layout of one texel of an atlas texture.
///
/// There are exactly two, and neither follows the output surface's format. That is what removes
/// the channel swizzle a surface-following colour format forces on every upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureFormat {
    /// One unsigned byte per texel, read as coverage in `0.0 ..= 1.0`.
    R8Unorm,
    /// Four unsigned bytes per texel in red, green, blue, alpha order.
    ///
    /// Colour is **premultiplied**: a texel's colour channels are already scaled by its alpha.
    /// Every consumer blends with a premultiplied blend factor, so straight-alpha bytes handed to
    /// [`Atlas::get_or_insert`](crate::Atlas::get_or_insert) would make a half-covered edge texel
    /// contribute its colour at full intensity — the soft-edge bloom and dark halo seen around
    /// avatars and emoji. Premultiplication belongs at decode time, once, before the bytes get
    /// here.
    Rgba8Unorm,
}

impl TextureFormat {
    /// How many bytes one texel occupies.
    pub const fn bytes_per_texel(self) -> u32 {
        match self {
            Self::R8Unorm => 1,
            Self::Rgba8Unorm => 4,
        }
    }

    /// How many bytes a `width` by `height` rectangle of these texels occupies, tightly packed.
    pub const fn bytes_for(self, width: u32, height: u32) -> u64 {
        (self.bytes_per_texel() as u64) * (width as u64) * (height as u64)
    }
}
