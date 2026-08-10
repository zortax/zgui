//! The four pools a tile can be allocated from.

use crate::texture::format::TextureFormat;

/// Which pool of textures a tile belongs to.
///
/// The set is closed and small on purpose: a pool is a *pipeline* distinction, not a content one.
/// What kind of content a tile holds is the caller's business and travels in the opaque
/// [`AtlasKey`](crate::AtlasKey) instead, so a consumer that caches a fifth kind of raster picks
/// the pool whose format and pipeline suit it rather than adding a variant here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureKind {
    /// Single-channel coverage: glyph masks, and vector shapes rasterised as alpha masks.
    Mono,
    /// Three-channel coverage for LCD-subpixel text, one coverage value per colour channel.
    ///
    /// Separate from [`TextureKind::Color`] despite sharing its format, because the two are drawn
    /// by different pipelines and a batch may not mix them.
    Subpixel,
    /// Full colour rasterised at the size it is drawn at: emoji, and colour glyphs.
    Color,
    /// Full colour drawn at whatever size layout says: decoded images.
    ///
    /// Separate from [`TextureKind::Color`] despite sharing its format and pipeline, because the
    /// two are *sampled* differently. A glyph is rasterised for its exact draw size and filtering
    /// would blur it; an image is decoded once and stretched to its box, and a device reads it
    /// through a filtering sampler or the stretch is blocky.
    Image,
}

impl TextureKind {
    /// How many kinds there are, which is how many pools an atlas keeps.
    pub const COUNT: usize = 4;

    /// Every kind, in pool order.
    pub const ALL: [Self; Self::COUNT] = [Self::Mono, Self::Subpixel, Self::Color, Self::Image];

    /// The pixel format textures of this kind are created with.
    ///
    /// Both formats are fixed rather than following the output surface. A pool whose format
    /// tracked the surface would need a CPU channel swizzle on every upload, and pinning the
    /// coverage pool to one byte per texel is what keeps a text-heavy frame's upload bandwidth
    /// down by a factor of four.
    pub const fn format(self) -> TextureFormat {
        match self {
            Self::Mono => TextureFormat::R8Unorm,
            Self::Subpixel | Self::Color | Self::Image => TextureFormat::Rgba8Unorm,
        }
    }

    /// The kind's position in the pool array.
    pub const fn index(self) -> usize {
        match self {
            Self::Mono => 0,
            Self::Subpixel => 1,
            Self::Color => 2,
            Self::Image => 3,
        }
    }
}
