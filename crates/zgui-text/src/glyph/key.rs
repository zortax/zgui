//! What one rasterised glyph is cached by.

use crate::font::face::FaceId;

/// How far into a pixel a glyph's origin falls, quantised.
///
/// Text is positioned in fractions of a pixel, and rasterising every fractional position would
/// defeat any cache. Quantising to quarters is what real rasterisers do: the error is a sixteenth
/// of a pixel at worst, invisible at reading sizes, and it turns an unbounded set of positions into
/// four.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubpixelOffset(pub u8);

impl SubpixelOffset {
    /// How many positions a pixel is divided into.
    pub const STEPS: u8 = 4;

    /// The offset a fractional position quantises to.
    ///
    /// This is half of a split and is of no use on its own: a fraction above the last step rounds
    /// up to the next whole pixel and so reports no offset at all, which is only right for a caller
    /// that draws the tile on that next pixel. The pixel and the offset together are
    /// [`PenPosition`](crate::PenPosition), which is what anything placing a glyph uses.
    ///
    /// ```
    /// use zgui_text::SubpixelOffset;
    ///
    /// assert_eq!(SubpixelOffset::quantise(12.0), SubpixelOffset(0));
    /// assert_eq!(SubpixelOffset::quantise(12.3), SubpixelOffset(1));
    /// assert_eq!(SubpixelOffset::quantise(-0.5), SubpixelOffset(2));
    /// ```
    pub fn quantise(position: f32) -> Self {
        crate::glyph::pen::PenPosition::of(position).offset()
    }

    /// The offset in pixels this stands for.
    pub fn to_pixels(self) -> f32 {
        f32::from(self.0) / f32::from(Self::STEPS)
    }
}

/// How a glyph is to be rasterised.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RasterStyle {
    /// One coverage value per pixel.
    Grayscale,
    /// Three coverage values per pixel, one per display subpixel, for horizontal RGB stripes.
    Subpixel,
    /// The face's own colour glyphs.
    Color,
}

/// Everything that decides what a rasterised glyph's pixels are.
///
/// Two requests with equal keys must produce identical pixels, because that is what makes it safe
/// to serve the second from a cache. The size is held as bits rather than as a number so that the
/// key can be hashed and compared exactly, which a float cannot be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// The face the glyph belongs to.
    pub face: FaceId,
    /// The glyph's index within that face — not a character, because one character may be several
    /// glyphs and one glyph may serve several characters.
    pub glyph: u16,
    /// The size in device pixels, as bits.
    pub size_bits: u32,
    /// Where in the pixel the glyph's origin falls.
    pub offset: SubpixelOffset,
    /// How it is to be rasterised.
    pub style: RasterStyle,
    /// Synthetic emboldening, as bits, for a weight no face covers.
    pub synthetic_bold_bits: u32,
    /// Synthetic slant in degrees, as bits, for an italic no face covers.
    pub synthetic_slant_bits: u32,
}

impl GlyphKey {
    /// A key with no synthesis, which is what a face that covers the requested style needs.
    pub fn new(
        face: FaceId,
        glyph: u16,
        size: f32,
        offset: SubpixelOffset,
        style: RasterStyle,
    ) -> Self {
        Self {
            face,
            glyph,
            size_bits: size.to_bits(),
            offset,
            style,
            synthetic_bold_bits: 0.0f32.to_bits(),
            synthetic_slant_bits: 0.0f32.to_bits(),
        }
    }

    /// The size in device pixels.
    pub fn size(&self) -> f32 {
        f32::from_bits(self.size_bits)
    }
}
