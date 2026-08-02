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
use crate::glyph::key::{GlyphKey, RasterStyle, SubpixelOffset};

/// How much a synthesised bold thickens a stem, as a fraction of the size it is drawn at.
///
/// A face that covers the requested weight is never emboldened, so this is only ever reached for a
/// family that ships one weight and was asked for another. The value is the one every engine
/// converges on: heavy enough to read as bold at text sizes, light enough not to fill a counter.
pub const SYNTHETIC_BOLD_RATIO: f32 = 0.02;

/// One glyph of a shaped line, positioned relative to the line box's top-left corner.
///
/// The position is where the glyph's *origin* falls — the point on the baseline the outline is
/// drawn from — not where its pixels start. Where the pixels start is a property of the
/// rasterisation and is reported by [`GlyphImage::placement`](crate::GlyphImage::placement),
/// because it depends on the size and the hinting and so cannot be known from the shaping alone.
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
/// one face, one size, one synthesis. It is also the unit a brush covers, which is why the slot is
/// here rather than on each glyph.
#[derive(Clone, Copy, Debug)]
pub struct ShapedRun<'a> {
    /// The face the glyphs belong to.
    pub face: FaceId,
    /// The size the run is drawn at, in device pixels.
    pub size: f32,
    /// How much the glyphs are emboldened to stand in for a weight the face does not have, as a
    /// fraction of the size; zero when the face covers the requested weight.
    pub synthetic_bold: f32,
    /// How far the glyphs are sheared to stand in for an italic the family does not have, in
    /// degrees; zero when a real italic was found.
    pub synthetic_slant: f32,
    /// Whether the face carries colour glyphs, which is what sends the run down the polychrome
    /// path rather than the coverage one.
    pub has_color: bool,
    /// The brush slot the run is drawn with.
    pub brush: Brush,
    /// The glyphs, in visual order.
    pub glyphs: &'a [ShapedGlyph],
}

impl ShapedRun<'_> {
    /// How this run's glyphs are to be rasterised.
    ///
    /// A colour face takes the colour path whatever else is true: its glyphs are pictures, and a
    /// coverage mask of a picture is a silhouette. Otherwise the caller's request decides, because
    /// whether per-channel coverage is meaningful is a property of the destination rather than of
    /// the text.
    ///
    /// ```
    /// use zgui_scene::PaintSlot;
    /// use zgui_text::{FaceId, RasterStyle, ShapedRun};
    ///
    /// let run = ShapedRun {
    ///     face: FaceId(0),
    ///     size: 16.0,
    ///     synthetic_bold: 0.0,
    ///     synthetic_slant: 0.0,
    ///     has_color: false,
    ///     brush: PaintSlot(0),
    ///     glyphs: &[],
    /// };
    /// assert_eq!(run.raster_style(true), RasterStyle::Subpixel);
    /// assert_eq!(run.raster_style(false), RasterStyle::Grayscale);
    ///
    /// let emoji = ShapedRun { has_color: true, ..run };
    /// assert_eq!(emoji.raster_style(true), RasterStyle::Color);
    /// ```
    pub fn raster_style(&self, subpixel: bool) -> RasterStyle {
        if self.has_color {
            RasterStyle::Color
        } else if subpixel {
            RasterStyle::Subpixel
        } else {
            RasterStyle::Grayscale
        }
    }

    /// The key one of this run's glyphs is rasterised and cached under, at a phase already split
    /// off its position.
    ///
    /// The position enters the key only through that phase, which is what makes the same glyph at
    /// the same phase one cache entry however many times it appears on the page — and what keeps a
    /// paragraph that scrolled by a whole pixel from re-rasterising.
    ///
    /// The phase is passed in rather than taken from the glyph because whoever draws the run holds
    /// the other half of the same split — the whole pixel the tile goes at — and the two must come
    /// from one [`PenPosition`](crate::PenPosition) or they describe different pixels.
    pub fn key_at(&self, glyph: u16, offset: SubpixelOffset, style: RasterStyle) -> GlyphKey {
        GlyphKey {
            face: self.face,
            glyph,
            size_bits: self.size.to_bits(),
            offset,
            style,
            synthetic_bold_bits: (self.synthetic_bold * self.size).to_bits(),
            synthetic_slant_bits: self.synthetic_slant.to_bits(),
        }
    }

    /// The key one of this run's glyphs is rasterised under when the line box's left edge is at
    /// `origin`.
    ///
    /// The origin is needed and is not an optional refinement: the phase is a property of where the
    /// glyph lands on the *surface*, and a line box that is itself at a fraction of a pixel shifts
    /// every phase on it. Passing the line-relative position alone rasterises for a phase the glyph
    /// is never drawn at.
    ///
    /// ```
    /// use zgui_scene::PaintSlot;
    /// use zgui_text::{FaceId, RasterStyle, ShapedGlyph, ShapedRun, SubpixelOffset};
    ///
    /// let run = ShapedRun {
    ///     face: FaceId(3),
    ///     size: 16.0,
    ///     synthetic_bold: 0.0,
    ///     synthetic_slant: 0.0,
    ///     has_color: false,
    ///     brush: PaintSlot(0),
    ///     glyphs: &[],
    /// };
    /// let here = run.key_for(ShapedGlyph { glyph: 9, x: 12.25, y: 0.0 }, 0.0, RasterStyle::Grayscale);
    /// let there = run.key_for(ShapedGlyph { glyph: 9, x: 40.25, y: 0.0 }, 0.0, RasterStyle::Grayscale);
    /// assert_eq!(here, there, "the same glyph at the same phase is one entry");
    /// assert_eq!(here.offset, SubpixelOffset(1));
    ///
    /// let moved = run.key_for(ShapedGlyph { glyph: 9, x: 12.25, y: 0.0 }, 0.5, RasterStyle::Grayscale);
    /// assert_eq!(moved.offset, SubpixelOffset(3), "a line box at half a pixel moves every phase");
    /// ```
    pub fn key_for(&self, glyph: ShapedGlyph, origin_x: f32, style: RasterStyle) -> GlyphKey {
        let offset = crate::glyph::pen::PenPosition::of(origin_x + glyph.x).offset();
        self.key_at(glyph.glyph, offset, style)
    }

    /// The key one of this run's glyphs' curves are held under.
    ///
    /// No position enters it, because an outline is the same curve wherever it is drawn: a run
    /// that scrolled, moved or turned asks for exactly the entry it asked for last frame.
    pub fn outline_key(&self, glyph: u16) -> crate::glyph::outline::OutlineKey {
        crate::glyph::outline::OutlineKey {
            face: self.face,
            glyph,
            size_bits: self.size.to_bits(),
            synthetic_slant_bits: self.synthetic_slant.to_bits(),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{ShapedGlyph, ShapedRun};
    use crate::glyph::key::RasterStyle;

    /// A run over one face at one size.
    fn run() -> ShapedRun<'static> {
        ShapedRun {
            face: crate::font::face::FaceId(1),
            size: 16.0,
            synthetic_bold: 0.0,
            synthetic_slant: 0.0,
            has_color: false,
            brush: zgui_scene::PaintSlot(0),
            glyphs: &[],
        }
    }

    #[test]
    fn two_phases_of_one_glyph_are_two_cache_entries() {
        let run = run();
        let flush = run.key_for(
            ShapedGlyph {
                glyph: 4,
                x: 10.0,
                y: 0.0,
            },
            0.0,
            RasterStyle::Grayscale,
        );
        let shifted = run.key_for(
            ShapedGlyph {
                glyph: 4,
                x: 10.5,
                y: 0.0,
            },
            0.0,
            RasterStyle::Grayscale,
        );
        assert_ne!(
            flush, shifted,
            "half a pixel of shift must be a different rasterisation"
        );
    }

    #[test]
    fn synthesis_is_part_of_what_a_glyph_is_cached_by() {
        let plain = run();
        let bold = ShapedRun {
            synthetic_bold: super::SYNTHETIC_BOLD_RATIO,
            ..plain
        };
        let glyph = ShapedGlyph {
            glyph: 4,
            x: 0.0,
            y: 0.0,
        };
        assert_ne!(
            plain.key_for(glyph, 0.0, RasterStyle::Grayscale),
            bold.key_for(glyph, 0.0, RasterStyle::Grayscale),
        );
    }
}
