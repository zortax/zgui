//! Positioned glyphs: what a shaped line hands to whoever draws it.
//!
//! Everything here is engine-neutral. A shaped paragraph's internal form belongs to the engine
//! that produced it and is never opened outside it; this is the narrow view of that form which
//! painting needs — for each style-uniform run of a line, the face, the size, the synthesis, the
//! brush, and where each glyph's origin falls.
//!
//! # The coordinate the positions are in
//!
//! Glyph positions are relative to the **line box's own top-left corner**, in device pixels, and
//! never to the paragraph or to the surface. That is what lets one shaped paragraph be drawn at
//! any position without being re-walked: whoever draws it adds the line box's absolute origin,
//! which it already has from the fragment tree, and a paragraph that scrolled by a pixel costs
//! nothing.

use crate::brush::Brush;
use crate::font::face::FaceId;

/// How much a synthesised bold thickens a stem, as a fraction of the size it is drawn at.
///
/// A face that covers the requested weight is never emboldened, so this is only ever reached for a
/// family that ships one weight and was asked for another. The value is the one every engine
/// converges on: heavy enough to read as bold at text sizes, light enough not to fill a counter.
pub const SYNTHETIC_BOLD_RATIO: f32 = 0.02;

/// The largest size, in device pixels, a run is drawn at with one coverage for each colour channel.
///
/// Per-channel coverage sharpens thin stems. A stem at a larger size is several pixels wide, so
/// the coverage adds no detail there and the tint along each long edge becomes visible. A larger
/// run gets one coverage for each pixel.
///
/// Stated in device pixels, because the width of a stem in device pixels decides this.
pub const SUBPIXEL_MAX_SIZE: f32 = 48.0;

/// One glyph of a shaped line, positioned relative to the line box's top-left corner.
///
/// The position is where the glyph's *origin* falls: the point on the baseline the outline is
/// drawn from. Where the pixels start depends on the size and the hinting, so the rasteriser
/// decides it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    /// The glyph's index within the run's face — not a character.
    pub glyph: u16,
    /// Distance from the line box's left edge to the glyph's origin, in device pixels.
    pub x: f32,
    /// Distance from the line box's top edge down to the baseline the glyph sits on, in device
    /// pixels.
    pub y: f32,
}

/// One style-uniform run of one shaped line.
///
/// A run is the unit a rasteriser can serve without changing anything about how it is configured:
/// one face, one size, one instance, one synthesis. It is also the unit a brush covers, which is
/// why the slot is here rather than on each glyph.
#[derive(Clone, Copy, Debug)]
pub struct ShapedRun<'a> {
    /// The face the glyphs belong to.
    pub face: FaceId,
    /// The size the run is drawn at, in device pixels.
    pub size: f32,
    /// The normalised variation coordinates the glyphs are drawn at, one per axis of the face in
    /// F2Dot14.
    ///
    /// Empty for a static face and for the default instance of a variable one.
    pub coords: &'a [i16],
    /// How much the glyphs are emboldened to stand in for a weight the face does not have, as a
    /// fraction of the size; zero when the face covers the requested weight.
    pub synthetic_bold: f32,
    /// How far the glyphs are sheared to stand in for an italic the family does not have, in
    /// degrees; zero when a real italic was found.
    pub synthetic_slant: f32,
    /// The brush slot the run is drawn with.
    pub brush: Brush,
    /// The glyphs, in visual order.
    pub glyphs: &'a [ShapedGlyph],
    /// The byte offset in the generated string where the run's text starts.
    ///
    /// It says which nested inline box the run belongs to. No cache key holds it.
    pub text_start: u32,
}

/// Where a shaped paragraph's positioned glyphs come from.
///
/// Kept apart from the shaper itself so that whoever draws a frame does not have to hold the
/// engine mutably while it does: shaping is a write and reading positioned glyphs is not, and a
/// seam that fused them would make painting and measuring compete for the same borrow.
pub trait ShapedGlyphs {
    /// Visits each style-uniform run of one line, in the order the runs are drawn.
    ///
    /// A paragraph that was never shaped, a line index past the last line, and a line with no
    /// glyphs on it all visit nothing, which is the same answer and is drawn the same way.
    fn visit_line(
        &self,
        paragraph: crate::paragraph::key::ParagraphKey,
        line: u16,
        visit: &mut dyn FnMut(ShapedRun<'_>),
    );
}
