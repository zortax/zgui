//! Repacking a rasterised glyph into the pixel layouts the rest of the pipeline uses.

use swash::scale::image::{Content, Image};
use zgui_geom::{DevicePx, Point, Size};
use zgui_text::{GlyphFormat, GlyphImage};

/// Converts one rasterised glyph.
///
/// Two of the three formats are a straight copy and the third is not: the rasteriser reports
/// subpixel coverage as four bytes per pixel with the fourth unused, while a subpixel glyph is
/// three coverage values. Carrying the padding byte all the way to the texture would cost a third
/// of the upload bandwidth of every text-heavy frame for nothing, so it is dropped here, once.
pub(crate) fn convert(image: &Image) -> GlyphImage {
    let size = Size::new(image.placement.width, image.placement.height);
    let placement = Point::new(
        DevicePx(image.placement.left as f32),
        DevicePx(image.placement.top as f32),
    );
    let (format, bytes) = match image.content {
        Content::Mask => (GlyphFormat::Mono, image.data.clone()),
        Content::SubpixelMask => (GlyphFormat::Subpixel, drop_fourth(&image.data)),
        Content::Color => (GlyphFormat::Color, image.data.clone()),
    };
    GlyphImage {
        size,
        placement,
        format,
        bytes,
    }
}

/// Keeps the first three bytes of every four.
fn drop_fourth(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 4 * 3);
    for pixel in data.chunks_exact(4) {
        out.extend_from_slice(&pixel[..3]);
    }
    out
}
