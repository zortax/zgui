//! Decoding raster images into the one form the rest of the framework accepts.
//!
//! Everything downstream of this crate — the content cache, the atlas, the colour-sprite pipeline
//! — speaks exactly one pixel format: premultiplied, gamma-encoded sRGB, four bytes per texel,
//! tightly packed, top row first. This crate is where a PNG, a JPEG, a WebP or a GIF becomes that,
//! and it is the only crate in the workspace that links a codec.
//!
//! # What this deliberately is not
//!
//! There is no I/O policy here beyond reading a path, no cache, no thread and no clock. Decoding
//! is CPU work that belongs off the frame thread, but *whose* thread is the runtime's decision —
//! the loader calls these functions from wherever it schedules blocking work. Keeping this crate
//! pure is what keeps it testable byte-for-byte.
//!
//! # Size limits are enforced by scaling, not refusal
//!
//! The atlas refuses content larger than the device's texture limit, and an image that was
//! refused would silently draw nothing. So the limit arrives *here*, as [`Limits`], and an image
//! over it is downscaled proportionally — before premultiplying, on the decoded values — and its
//! reported extent is the downscaled one. A caller that wants full-resolution zoomable imagery is
//! outside what an `image` element promises and brings its own texture.

#![forbid(unsafe_code)]

use std::path::Path;
use std::sync::Arc;

use zgui_geom::{Device, Size};

/// One decoded image, in the framework's pixel format.
#[derive(Clone, Debug)]
pub struct Decoded {
    /// The pixel extent, after any downscale [`Limits`] forced.
    pub size: Size<u32, Device>,
    /// The texels: premultiplied, gamma-encoded sRGB, RGBA, four bytes per texel, tightly packed,
    /// top row first. Behind an [`Arc`] because one decode is shown by any number of elements and
    /// held by a cache besides.
    pub texels: Arc<Vec<u8>>,
}

/// What a decode is allowed to produce.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The largest extent either axis may have. An image over it is downscaled proportionally
    /// until both axes fit.
    pub max_dimension: u32,
}

impl Default for Limits {
    /// The default is the smallest texture limit any supported device guarantees, so a decode
    /// bounded by it fits every atlas.
    fn default() -> Self {
        Self {
            max_dimension: 2048,
        }
    }
}

/// What went wrong decoding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DecodeError {
    /// The file could not be read at all.
    #[error("could not read the image file: {0}")]
    Io(#[from] std::io::Error),
    /// The bytes are not a format this build links, or not an image at all.
    #[error("could not decode the image: {0}")]
    Decode(#[from] image::ImageError),
    /// The image decoded to no pixels, which nothing downstream can draw.
    #[error("the image is empty")]
    Empty,
}

/// Decodes `bytes` into the framework's pixel format, downscaling to fit `limits`.
///
/// The format is sniffed from the bytes; a GIF decodes to its first frame.
///
/// # Errors
///
/// [`DecodeError::Decode`] for bytes no linked codec accepts, [`DecodeError::Empty`] for an image
/// with a zero axis.
pub fn decode(bytes: &[u8], limits: Limits) -> Result<Decoded, DecodeError> {
    finish(image::load_from_memory(bytes)?, limits)
}

/// Decodes the file at `path`, sniffing the format from its content rather than its name.
///
/// # Errors
///
/// [`DecodeError::Io`] when the file cannot be read; otherwise as [`decode`].
pub fn decode_file(path: &Path, limits: Limits) -> Result<Decoded, DecodeError> {
    let bytes = std::fs::read(path)?;
    decode(&bytes, limits)
}

/// In-memory image bytes, addressable through a `src` string.
///
/// `src` is the one wire an image element has, and it carries text. Bytes an application already
/// holds — an asset compiled in, a thumbnail a protocol delivered — register here and travel as
/// [`ImageBytes::url`], a `zgui-bytes:` URL the loader resolves back through [`bytes_for_url`].
///
/// The handle is the registration: dropping the last clone unregisters the bytes, and a URL that
/// outlives its handle decodes to nothing. Hold the handle at least as long as anything shows the
/// picture — an eviction re-decodes through the same URL.
#[derive(Clone, Debug)]
pub struct ImageBytes {
    /// The registry slot, shared so clones keep the registration alive together.
    slot: Arc<Registration>,
}

/// One registered buffer; the drop of the last handle is what removes it.
#[derive(Debug)]
struct Registration {
    /// The registry key.
    token: u64,
}

/// The scheme [`ImageBytes::url`] speaks.
const BYTES_SCHEME: &str = "zgui-bytes:";

/// The registered buffers, by token.
static REGISTRY: std::sync::Mutex<Option<std::collections::HashMap<u64, Arc<Vec<u8>>>>> =
    std::sync::Mutex::new(None);

impl ImageBytes {
    /// Registers `bytes` and returns the handle that keeps them addressable.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let token = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        REGISTRY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert_with(Default::default)
            .insert(token, Arc::new(bytes.into()));
        Self {
            slot: Arc::new(Registration { token }),
        }
    }

    /// The `src` value that names these bytes.
    pub fn url(&self) -> String {
        format!("{BYTES_SCHEME}{}", self.slot.token)
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(registry) = REGISTRY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
        {
            registry.remove(&self.token);
        }
    }
}

/// Resolves a `zgui-bytes:` URL to the registered buffer, if the URL is one and the handle lives.
///
/// `None` for any other string, which is how a loader asks "is this in-memory?" before treating
/// the source as a path.
pub fn bytes_for_url(url: &str) -> Option<Arc<Vec<u8>>> {
    let token = url.strip_prefix(BYTES_SCHEME)?.parse::<u64>().ok()?;
    REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()?
        .get(&token)
        .cloned()
}

/// Downscales, converts and premultiplies one decoded image.
fn finish(decoded: image::DynamicImage, limits: Limits) -> Result<Decoded, DecodeError> {
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 {
        return Err(DecodeError::Empty);
    }
    let max = limits.max_dimension.max(1);
    let decoded = if width > max || height > max {
        // Lanczos, because a downscale forced by a texture limit is permanent: the discarded
        // resolution is never coming back, so it is worth the best filter the codec crate has.
        decoded.resize(max, max, image::imageops::FilterType::Lanczos3)
    } else {
        decoded
    };

    let size = Size::new(decoded.width(), decoded.height());
    let mut texels = decoded.into_rgba8().into_raw();
    premultiply(&mut texels);
    Ok(Decoded {
        size,
        texels: Arc::new(texels),
    })
}

/// Premultiplies straight-alpha RGBA in place, on the gamma-encoded values.
///
/// On the encoded values by design: the whole pipeline composites in gamma space — see the
/// renderer's format table — so premultiplying after a decode to linear light would bake in a
/// double conversion. Rounding is to nearest, which keeps `premultiply` idempotent for the two
/// alphas that matter most: 0 and 255.
fn premultiply(texels: &mut [u8]) {
    for texel in texels.chunks_exact_mut(4) {
        let alpha = u16::from(texel[3]);
        if alpha == 255 {
            continue;
        }
        for channel in &mut texel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageEncoder, codecs::png::PngEncoder};

    use super::*;

    /// Encodes a tiny straight-alpha RGBA image as a PNG.
    fn png(width: u32, height: u32, texels: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        PngEncoder::new(&mut out)
            .write_image(texels, width, height, image::ExtendedColorType::Rgba8)
            .expect("encoding a well-formed buffer succeeds");
        out
    }

    #[test]
    fn a_png_round_trips_into_premultiplied_texels() {
        // Opaque red, transparent green, half-transparent white, opaque black.
        let straight: [u8; 16] = [
            255, 0, 0, 255, //
            0, 255, 0, 0, //
            255, 255, 255, 128, //
            0, 0, 0, 255,
        ];
        let decoded = decode(&png(4, 1, &straight), Limits::default()).expect("decodes");
        assert_eq!(decoded.size, Size::new(4, 1));
        assert_eq!(
            decoded.texels.as_slice(),
            &[
                255, 0, 0, 255, // opaque survives untouched
                0, 0, 0, 0, // fully transparent premultiplies to nothing
                128, 128, 128, 128, // half alpha halves every channel, to nearest
                0, 0, 0, 255,
            ],
            "the texels are premultiplied on the encoded values"
        );
    }

    #[test]
    fn the_byte_count_matches_the_extent_the_cache_will_check() {
        let texels = vec![7u8; 5 * 3 * 4];
        let decoded = decode(&png(5, 3, &texels), Limits::default()).expect("decodes");
        assert_eq!(
            decoded.texels.len(),
            decoded.size.width as usize * decoded.size.height as usize * 4,
            "this is the exact invariant ContentCache::set_image_shared refuses on"
        );
    }

    #[test]
    fn an_oversized_image_is_downscaled_to_fit_and_reports_the_downscaled_extent() {
        let texels = vec![200u8; 64 * 16 * 4];
        let decoded = decode(&png(64, 16, &texels), Limits { max_dimension: 32 }).expect("decodes");
        assert_eq!(
            decoded.size,
            Size::new(32, 8),
            "the downscale is proportional and the reported extent is the truth about the texels"
        );
        assert_eq!(decoded.texels.len(), 32 * 8 * 4);
    }

    #[test]
    fn registered_bytes_resolve_until_the_last_handle_drops() {
        let bytes = ImageBytes::new(vec![1u8, 2, 3]);
        let url = bytes.url();
        assert!(url.starts_with("zgui-bytes:"));
        assert_eq!(
            bytes_for_url(&url).as_deref().map(Vec::as_slice),
            Some([1u8, 2, 3].as_slice())
        );
        let clone = bytes.clone();
        drop(bytes);
        assert!(bytes_for_url(&url).is_some(), "a clone keeps it registered");
        drop(clone);
        assert!(bytes_for_url(&url).is_none(), "the last drop unregisters");
        assert!(bytes_for_url("photo.png").is_none(), "paths are not urls");
    }

    #[test]
    fn garbage_is_a_decode_error_and_an_empty_image_is_its_own() {
        assert!(matches!(
            decode(b"not an image", Limits::default()),
            Err(DecodeError::Decode(_))
        ));
        // A 0x0 PNG cannot be encoded, so exercise the guard directly.
        let empty = image::DynamicImage::new_rgba8(0, 0);
        assert!(matches!(
            finish(empty, Limits::default()),
            Err(DecodeError::Empty)
        ));
    }
}
