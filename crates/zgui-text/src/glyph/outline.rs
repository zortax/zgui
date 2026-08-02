//! The curves one glyph is made of, for the runs the atlas cannot serve.
//!
//! # The space the curves are in
//!
//! Device pixels, y growing **downward**, with the glyph's origin on the baseline at zero. That is
//! the same space a placed glyph's rectangle is in, so drawing one is a translation by where the
//! glyph's origin falls and nothing else — no flip, no scale, no per-consumer convention to get
//! wrong. A face's own outlines are the other way up and in font units; converting them is the
//! provider's job precisely so that it is done once.
//!
//! # What is not in the key
//!
//! The position is not, because an outline is the same curve wherever it is drawn — there is no
//! phase to rasterise for, which is why the whole subpixel-offset apparatus is absent here. Neither
//! is a synthesised bold, because emboldening an outline is a stroke around it: whoever fills the
//! curve strokes it in the same brush, and the curve itself is the one the face draws.

use std::sync::Arc;

use kurbo::BezPath;

use crate::font::face::FaceId;

/// Everything that decides what one glyph's curves are.
///
/// Two requests with equal keys must produce the same curves, and a provider that caches is
/// expected to hand back the *same allocation* — a path rasteriser keeps its encoding of a path
/// under the identity of the allocation, so a fresh copy per frame is a re-encode per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutlineKey {
    /// The face the glyph belongs to.
    pub face: FaceId,
    /// The glyph's index within that face.
    pub glyph: u16,
    /// The size in device pixels, as bits, so that the key hashes and compares exactly.
    pub size_bits: u32,
    /// Synthetic slant in degrees, as bits, for an italic no face covers.
    ///
    /// Part of the curve rather than of the transform: the shear is about each glyph's own origin,
    /// so a run cannot carry it and a consumer that applied it to the run would lean the line
    /// rather than the letters.
    pub synthetic_slant_bits: u32,
}

impl OutlineKey {
    /// A key with no synthesis, which is what a face that covers the requested style needs.
    pub fn new(face: FaceId, glyph: u16, size: f32) -> Self {
        Self {
            face,
            glyph,
            size_bits: size.to_bits(),
            synthetic_slant_bits: 0.0f32.to_bits(),
        }
    }

    /// The size in device pixels.
    pub fn size(&self) -> f32 {
        f32::from_bits(self.size_bits)
    }

    /// The slant in degrees.
    pub fn synthetic_slant(&self) -> f32 {
        f32::from_bits(self.synthetic_slant_bits)
    }
}

/// One glyph's curves, shared so that the same allocation reaches a rasteriser every frame.
pub type GlyphOutline = Arc<BezPath>;
