//! Turning one glyph into pixels.

use std::sync::{Arc, Mutex};

use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Angle, Format, Transform, Vector};
use zgui_text::{FaceId, GlyphImage, GlyphKey, GlyphOutline, GlyphRaster, OutlineKey, RasterStyle};

use crate::raster::faces::FaceBytes;
use crate::raster::image::convert;
use crate::raster::outline::Outlines;
use crate::system::FontSystem;

/// The sources a monochrome or subpixel request draws from, in order of preference.
///
/// An outline first, then an alpha bitmap strike for the faces that ship one instead — a bitmap-only
/// face would otherwise rasterise to nothing at all.
const MASK_SOURCES: [Source; 2] = [Source::Outline, Source::Bitmap(StrikeWith::BestFit)];

/// The sources a colour request draws from, in order of preference.
///
/// Layered colour outlines first, then a colour bitmap strike, then the plain outline. The last is
/// what makes a colour request against an ordinary text face produce the glyph in a single colour
/// rather than produce nothing.
const COLOR_SOURCES: [Source; 3] = [
    Source::ColorOutline(0),
    Source::ColorBitmap(StrikeWith::BestFit),
    Source::Outline,
];

/// Rasterises glyphs from the faces one font system holds.
///
/// Hinted, and capable of subpixel coverage and of the face's own colour glyphs. Which of those a
/// glyph gets is entirely in the key it is asked for, never in state held here — a hinting setting
/// kept on the side would silently change what an already cached key meant.
///
/// ```
/// use std::sync::Arc;
/// use zgui_text::{GlyphKey, GlyphRaster, FaceId, RasterStyle, SubpixelOffset};
/// use zgui_text_parley::{FontSystem, FontSystemOptions, Rasteriser};
///
/// let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
/// let raster = Rasteriser::new(fonts);
///
/// // A handle no system issued has no glyph, which is not the same as a blank one.
/// let key = GlyphKey::new(FaceId(7), 1, 16.0, SubpixelOffset(0), RasterStyle::Grayscale);
/// assert!(raster.raster(&key).is_none());
/// ```
pub struct Rasteriser {
    /// The system the faces come from.
    fonts: Arc<FontSystem>,
    /// Face bytes already looked up.
    faces: FaceBytes,
    /// The rasteriser's own caches and scratch buffers.
    scale: Mutex<ScaleContext>,
    /// The curves already extracted, for the runs the atlas cannot serve.
    outlines: Outlines,
}

impl Rasteriser {
    /// A rasteriser over one font system's faces.
    pub fn new(fonts: Arc<FontSystem>) -> Self {
        Self {
            fonts,
            faces: FaceBytes::default(),
            scale: Mutex::new(ScaleContext::new()),
            outlines: Outlines::default(),
        }
    }
}

impl GlyphRaster for Rasteriser {
    fn raster(&self, key: &GlyphKey) -> Option<GlyphImage> {
        let (blob, index) = self.faces.get(&self.fonts, key.face)?;
        let font = FontRef::from_index(blob.data(), index as usize)?;
        let mut context = self
            .scale
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut scaler = context
            .builder_with_id(font, identity(key.face, index))
            .size(key.size())
            .hint(true)
            .build();
        let image = renderer(key).render(&mut scaler, key.glyph)?;
        Some(convert(&image))
    }

    fn outline(&self, key: &OutlineKey) -> Option<GlyphOutline> {
        let (blob, index) = self.faces.get(&self.fonts, key.face)?;
        // Read through a second reader rather than the one the bitmap path uses: the curves are
        // wanted unhinted and unshifted, and the scaler that produces tiles is configured for
        // neither.
        let font = skrifa::FontRef::from_index(blob.data(), index).ok()?;
        self.outlines.get(&font, key)
    }
}

/// What the scaler caches one face's prepared state under.
///
/// It has to be *stable* across calls and *distinct* between faces, and it is neither by default:
/// a font reference built from bytes mints a fresh identity every time it is constructed, so a
/// scaler built per glyph shares nothing with the one before it and runs the face's `fpgm` and
/// `prep` control programs again for every single glyph. A face handle and the index within its
/// collection are exactly the pair that names a face, so they are what the identity is made of.
fn identity(face: FaceId, index: u32) -> [u64; 2] {
    [u64::from(face.0), u64::from(index)]
}

/// The render configured for one key.
fn renderer(key: &GlyphKey) -> Render<'static> {
    let sources: &'static [Source] = match key.style {
        RasterStyle::Color => &COLOR_SOURCES,
        RasterStyle::Grayscale | RasterStyle::Subpixel => &MASK_SOURCES,
    };
    let mut render = Render::new(sources);
    render.format(match key.style {
        RasterStyle::Subpixel => Format::Subpixel,
        RasterStyle::Grayscale | RasterStyle::Color => Format::Alpha,
    });
    // Positioning is in fractions of a pixel and the key carries the phase, so the same glyph at
    // four phases is four cache entries rather than an unbounded set.
    render.offset(Vector::new(key.offset.to_pixels(), 0.0));
    let bold = f32::from_bits(key.synthetic_bold_bits);
    if bold != 0.0 {
        render.embolden(bold);
    }
    let slant = f32::from_bits(key.synthetic_slant_bits);
    if slant != 0.0 {
        // A synthesised italic leans the glyph forward, which is a shear along x by the angle the
        // face would have been drawn at.
        render.transform(Some(Transform::skew(
            Angle::from_degrees(-slant),
            Angle::ZERO,
        )));
    }
    render
}
