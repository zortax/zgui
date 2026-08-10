//! The fused decode path: decoder output lands in the final buffer.
//!
//! The general path builds a [`image::DynamicImage`] in the source's own colour type and converts
//! it to RGBA afterwards, which is a second full-image allocation and pass. The two colour types
//! that cover the common formats — `Rgba8` for PNG-with-alpha, WebP-with-alpha and GIF, `Rgb8` for
//! JPEG and PNG-without-alpha — can decode straight into the buffer the framework keeps:
//!
//! - `Rgba8` decodes into the final buffer and premultiplies in place. One allocation.
//! - `Rgb8` decodes into the tail of the final buffer and expands to RGBA in place, front to
//!   back. One allocation, and no premultiply pass at all: the pixels are opaque.
//!
//! Everything else — greyscale, sixteen-bit channels, an image over the size limit — takes the
//! general path, where the conversion copy is the cost of not hand-writing every converter.
//!
//! EXIF orientation is not applied anywhere in this crate yet. When it is, it belongs here: the
//! decoder held by this module exposes [`image::ImageDecoder::orientation`], and a rotation fused
//! into this pass is the only rotation that costs no extra buffer.

use std::io::Cursor;

use image::{ColorType, ImageDecoder, ImageReader};
use zgui_geom::Size;

use crate::{DecodeError, Decoded};

/// Decodes `bytes` with the fewest passes the source's colour type allows, downscaling until the
/// long edge fits `max_long_edge`.
pub(crate) fn decode(bytes: &[u8], max_long_edge: u32) -> Result<Decoded, DecodeError> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err(DecodeError::Empty);
    }
    let max = max_long_edge.max(1);
    if width > max || height > max {
        // The downscale needs the whole image in memory anyway, so the general path costs
        // nothing extra here.
        return crate::finish(image::DynamicImage::from_decoder(decoder)?, max);
    }
    let count = width as usize * height as usize;
    match decoder.color_type() {
        ColorType::Rgba8 => {
            let mut texels = vec![0u8; count * 4];
            decoder.read_image(&mut texels)?;
            crate::premultiply(&mut texels);
            Ok(Decoded {
                size: Size::new(width, height),
                texels: std::sync::Arc::new(texels),
            })
        }
        ColorType::Rgb8 => {
            let mut texels = vec![0u8; count * 4];
            decoder.read_image(&mut texels[count..])?;
            expand_rgb(&mut texels, count);
            Ok(Decoded {
                size: Size::new(width, height),
                texels: std::sync::Arc::new(texels),
            })
        }
        _ => crate::finish(image::DynamicImage::from_decoder(decoder)?, max),
    }
}

/// Expands `count` RGB pixels, stored in the tail of `texels`, to RGBA over the whole buffer.
///
/// In place and front to back. Pixel `i` is read from `count + 3i` and written to `4i`; the write
/// ends at `4i + 4`, and the first source byte still unread is at `count + 3i + 3`, which is never
/// earlier — so the only overlap is with pixel `i`'s own source bytes, and reading them into
/// locals first is what makes that safe.
fn expand_rgb(texels: &mut [u8], count: usize) {
    debug_assert_eq!(texels.len(), count * 4);
    for i in 0..count {
        let [r, g, b]: [u8; 3] = texels[count + 3 * i..count + 3 * i + 3]
            .try_into()
            .expect("three bytes were sliced");
        texels[4 * i..4 * i + 4].copy_from_slice(&[r, g, b, 255]);
    }
}
