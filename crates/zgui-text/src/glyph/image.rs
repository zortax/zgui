//! The pixels one rasterised glyph occupies.

use zgui_geom::{Device, DevicePx, Point, Size};

/// How the bytes of a [`GlyphImage`] are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlyphFormat {
    /// One byte per pixel: coverage.
    Mono,
    /// Three bytes per pixel: coverage for each display subpixel, in red, green, blue order.
    Subpixel,
    /// Four bytes per pixel: straight-alpha, gamma-encoded sRGB.
    Color,
}

impl GlyphFormat {
    /// How many bytes one pixel takes.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Subpixel => 3,
            Self::Color => 4,
        }
    }
}

/// One rasterised glyph: its pixels, and where they sit relative to the glyph's origin.
///
/// The origin is the point on the baseline the glyph is placed at. [`GlyphImage::placement`] is the
/// top-left corner of the pixels relative to it, so it is normally above the baseline and to the
/// right — but not always, which is why it is signed in both axes rather than a pair of bearings.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphImage {
    /// The extent of the rasterised area.
    pub size: Size<u32, Device>,
    /// The top-left corner of that area, relative to the glyph's origin on the baseline.
    pub placement: Point<DevicePx, Device>,
    /// How the bytes are laid out.
    pub format: GlyphFormat,
    /// The pixels, row by row with no padding between rows.
    pub bytes: Vec<u8>,
}

impl GlyphImage {
    /// Whether the byte count matches the extent and the format.
    ///
    /// A rasteriser that returns a mismatched image would have whatever consumes it read past the
    /// end of a row, so a consumer checks rather than assumes.
    pub fn is_well_formed(&self) -> bool {
        let pixels = self.size.width as usize * self.size.height as usize;
        self.bytes.len() == pixels * self.format.bytes_per_pixel()
    }

    /// Whether the glyph rasterised to nothing, which a space does.
    pub fn is_empty(&self) -> bool {
        self.size.width == 0 || self.size.height == 0
    }
}
