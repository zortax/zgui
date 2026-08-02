//! The seam a glyph rasteriser is plugged in through.

use crate::glyph::image::GlyphImage;
use crate::glyph::key::GlyphKey;
use crate::glyph::outline::{GlyphOutline, OutlineKey};

/// Turns a glyph into pixels.
///
/// The contract is deliberately one glyph at a time and entirely by value: nothing here knows about
/// atlases, textures or frames, so a rasteriser can be exercised against a byte buffer with no GPU
/// anywhere. Where the pixels then go — an atlas tile, a vector scene, a file on disk — is the
/// caller's question.
///
/// # What an implementation must promise
///
/// Equal keys give identical bytes. That is the property a cache between this trait and its caller
/// depends on, and it is why every input a rasteriser reads is in [`GlyphKey`] rather than in the
/// rasteriser's own state — a hinting setting held on the side would silently change what a cached
/// key means.
pub trait GlyphRaster: Send + Sync + 'static {
    /// Rasterises one glyph, or reports that the face has no outline for it.
    ///
    /// A glyph that is genuinely blank — a space — rasterises to a well-formed image of zero
    /// extent, which is not the same answer as the face not having the glyph at all.
    fn raster(&self, key: &GlyphKey) -> Option<GlyphImage>;

    /// The curves of one glyph, for a run the atlas cannot serve.
    ///
    /// Required rather than defaulted, and deliberately so. A rasteriser that could quietly answer
    /// *no outlines* would leave every rotated heading, every display size and every gradient run
    /// drawing nothing, with no error anywhere and every test still green — so a rasteriser that
    /// has faces at all has to say what it does about outlines, out loud, in its own source.
    ///
    /// The curves are in device pixels with y growing downward and the glyph's origin at zero; see
    /// [`OutlineKey`]. A blank glyph answers with an empty path, which is not the same answer as
    /// the face not having the glyph.
    fn outline(&self, key: &OutlineKey) -> Option<GlyphOutline>;
}

/// A rasteriser that has no faces and so draws nothing.
///
/// What a window brought up before a font engine has been chosen draws its text through. It
/// reports every glyph as absent rather than as blank, which is the honest answer from something
/// that holds no face: a blank image would claim the face was consulted and had nothing there.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRaster;

impl GlyphRaster for NoRaster {
    fn raster(&self, _key: &GlyphKey) -> Option<GlyphImage> {
        None
    }

    fn outline(&self, _key: &OutlineKey) -> Option<GlyphOutline> {
        None
    }
}
