//! Getting a rasterised glyph into an atlas tile.

use std::hash::{Hash, Hasher};

use crate::glyph::image::{GlyphFormat, GlyphImage};
use crate::glyph::key::GlyphKey;
use rustc_hash::FxHasher;
use zgui_atlas::{AtlasKey, TextureKind};
use zgui_geom::{Device, Size};

/// One glyph, in the form an atlas takes.
///
/// The two conversions this performs are the ones that go wrong silently. A subpixel glyph is
/// three coverage values per pixel and the atlas pool that draws it is four bytes wide, so the
/// bytes are padded; a colour glyph arrives with straight alpha and every atlas pool is
/// premultiplied, so its colour channels are scaled by its alpha. Uploading straight bytes instead
/// makes every soft edge — the outline of an emoji, the edge of an icon — bloom light, which no
/// invariant test can see and every user can.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtlasGlyph {
    /// What the atlas caches the tile under.
    pub key: AtlasKey,
    /// The extent of the tile.
    pub size: Size<u32, Device>,
    /// The tile's texels, premultiplied and tightly packed.
    pub texels: Vec<u8>,
}

impl AtlasGlyph {
    /// Prepares one rasterised glyph for the pool its format belongs in.
    ///
    /// ```
    /// use zgui_atlas::TextureKind;
    /// use zgui_geom::{DevicePx, Point, Size};
    /// use zgui_text::{
    ///     AtlasGlyph, FaceId, GlyphFormat, GlyphImage, GlyphKey, RasterStyle, SubpixelOffset,
    /// };
    ///
    /// let image = GlyphImage {
    ///     size: Size::new(1, 1),
    ///     placement: Point::new(DevicePx(0.0), DevicePx(0.0)),
    ///     format: GlyphFormat::Color,
    ///     bytes: vec![255, 0, 0, 128],
    /// };
    /// let key = GlyphKey::new(FaceId(0), 3, 16.0, SubpixelOffset(0), RasterStyle::Color);
    ///
    /// let tile = AtlasGlyph::of(&key, &image);
    /// assert_eq!(tile.key.kind(), TextureKind::Color);
    /// assert_eq!(tile.texels, vec![128, 0, 0, 128], "straight alpha is premultiplied");
    /// ```
    pub fn of(key: &GlyphKey, image: &GlyphImage) -> Self {
        Self {
            key: AtlasKey::new(handle(key), kind(image.format)),
            size: image.size,
            texels: texels(image),
        }
    }
}

/// Which pool a glyph's format belongs in.
pub fn kind(format: GlyphFormat) -> TextureKind {
    match format {
        GlyphFormat::Mono => TextureKind::Mono,
        GlyphFormat::Subpixel => TextureKind::Subpixel,
        GlyphFormat::Color => TextureKind::Color,
    }
}

/// The opaque handle an atlas caches a glyph under.
///
/// Every input that changes the pixels is in the glyph key already, so the handle is a hash of the
/// whole of it: two requests that would rasterise identically share a tile, and two that would not
/// never can.
pub fn handle(key: &GlyphKey) -> u64 {
    let mut hasher = FxHasher::default();
    key.hash(&mut hasher);
    // The high byte is reserved for non-glyph monochrome rasters (small vector masks). Keeping
    // the namespaces disjoint makes sharing one atlas exact rather than relying on a hash not to
    // collide with an independently allocated mask handle.
    hasher.finish() & 0x00FF_FFFF_FFFF_FFFF
}

/// The texels for one glyph, in its pool's format.
fn texels(image: &GlyphImage) -> Vec<u8> {
    match image.format {
        GlyphFormat::Mono => image.bytes.clone(),
        GlyphFormat::Subpixel => pad_to_four(&image.bytes),
        GlyphFormat::Color => premultiply(&image.bytes),
    }
}

/// Widens three coverage bytes per pixel to four, with an opaque fourth.
///
/// The fourth channel is not a coverage value and is never read as one: a subpixel tile carries
/// one coverage per colour channel and the pipeline that draws it reads three.
fn pad_to_four(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() / 3 * 4);
    for pixel in bytes.chunks_exact(3) {
        out.extend_from_slice(pixel);
        out.push(u8::MAX);
    }
    out
}

/// Scales each colour channel by its own alpha.
fn premultiply(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for pixel in bytes.chunks_exact(4) {
        let alpha = u32::from(pixel[3]);
        for channel in &pixel[..3] {
            out.push(((u32::from(*channel) * alpha + 127) / 255) as u8);
        }
        out.push(pixel[3]);
    }
    out
}
