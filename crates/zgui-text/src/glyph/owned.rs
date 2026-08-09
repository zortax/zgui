//! A shaped run held apart from the shaping that produced it.
//!
//! [`ShapedRun`] borrows the glyphs it describes, which is what a paragraph wants: the engine owns
//! them, one visit reads them, and nothing is copied. Text that is shaped outside a paragraph wants
//! the opposite. A caller that shapes its own lines — a terminal, a code view, anything laying out
//! cells itself — meets the same line again on the frame after, and re-shaping it every time is the
//! largest avoidable cost such a thing has. This is that result, owned, so it can be held in a
//! cache and drawn from for as many frames as the line is unchanged.

use crate::brush::Brush;
use crate::font::face::FaceId;
use crate::glyph::run::{ShapedGlyph, ShapedRun};

/// One style-uniform run of one shaped line, owned.
///
/// The fields are [`ShapedRun`]'s, with two differences. The glyphs are owned rather than borrowed.
/// And there is no brush: a slot is a property of the scene the run is drawn into rather than of
/// the shaping, so it is supplied at [`as_run`](Self::as_run) time and one cached run can be drawn
/// in any colour.
///
/// `clusters` holds one entry per glyph and is therefore the same length as `glyphs`. Building one
/// with the two lengths apart is a bug in whoever built it; [`as_run`](Self::as_run) checks it in
/// debug builds.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedRunOwned {
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
    /// Whether the face carries colour glyphs.
    pub has_color: bool,
    /// The glyphs, in visual order, positioned against the line box's top-left corner.
    pub glyphs: Vec<ShapedGlyph>,
    /// Where in the shaped string each glyph came from: the byte the glyph's cluster starts at, one
    /// entry per glyph, in the same order as `glyphs`.
    ///
    /// Every glyph of a ligature reports the byte its cluster starts at, so a ligature of two
    /// characters drawn as one glyph reports one entry. The values descend through a
    /// right-to-left run, because the glyphs are stored in the order they are drawn.
    pub clusters: Vec<u32>,
}

impl ShapedRunOwned {
    /// The borrowed view, with the brush supplied by whoever draws it.
    ///
    /// Every key a run is rasterised under is built through this, so a cached run and a freshly
    /// shaped one produce the same cache entries.
    ///
    /// ```
    /// use zgui_scene::PaintSlot;
    /// use zgui_text::{FaceId, RasterStyle, ShapedGlyph, ShapedRun, ShapedRunOwned};
    ///
    /// let glyph = ShapedGlyph { glyph: 9, x: 12.25, y: 0.0 };
    /// let owned = ShapedRunOwned {
    ///     face: FaceId(3),
    ///     size: 16.0,
    ///     synthetic_bold: 0.0,
    ///     synthetic_slant: 0.0,
    ///     has_color: false,
    ///     glyphs: vec![glyph],
    ///     clusters: vec![0],
    /// };
    /// let borrowed = ShapedRun {
    ///     face: FaceId(3),
    ///     size: 16.0,
    ///     synthetic_bold: 0.0,
    ///     synthetic_slant: 0.0,
    ///     has_color: false,
    ///     brush: PaintSlot(0),
    ///     glyphs: &[glyph],
    /// };
    /// assert_eq!(
    ///     owned.as_run(PaintSlot(0)).key_for(glyph, 0.0, RasterStyle::Grayscale),
    ///     borrowed.key_for(glyph, 0.0, RasterStyle::Grayscale),
    /// );
    /// ```
    pub fn as_run(&self, brush: Brush) -> ShapedRun<'_> {
        debug_assert_eq!(
            self.glyphs.len(),
            self.clusters.len(),
            "one cluster byte per glyph"
        );
        ShapedRun {
            face: self.face,
            size: self.size,
            synthetic_bold: self.synthetic_bold,
            synthetic_slant: self.synthetic_slant,
            has_color: self.has_color,
            brush,
            glyphs: &self.glyphs,
        }
    }
}

impl From<ShapedRun<'_>> for ShapedRunOwned {
    /// Copies a borrowed run, with no cluster mapping.
    ///
    /// A run visited from a paragraph carries no cluster bytes — the paragraph answers that
    /// question through its own cluster seam — so `clusters` holds the byte each glyph's run starts
    /// at, which for a paragraph run is zero for every glyph.
    fn from(run: ShapedRun<'_>) -> Self {
        Self {
            face: run.face,
            size: run.size,
            synthetic_bold: run.synthetic_bold,
            synthetic_slant: run.synthetic_slant,
            has_color: run.has_color,
            glyphs: run.glyphs.to_vec(),
            clusters: vec![0; run.glyphs.len()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShapedRunOwned;
    use crate::font::face::FaceId;
    use crate::glyph::key::RasterStyle;
    use crate::glyph::run::{ShapedGlyph, ShapedRun};
    use zgui_scene::PaintSlot;

    /// An owned run over one face at one size.
    fn owned() -> ShapedRunOwned {
        ShapedRunOwned {
            face: FaceId(3),
            size: 16.0,
            synthetic_bold: 0.0,
            synthetic_slant: 0.0,
            has_color: false,
            glyphs: vec![
                ShapedGlyph {
                    glyph: 9,
                    x: 0.0,
                    y: 12.0,
                },
                ShapedGlyph {
                    glyph: 11,
                    x: 8.5,
                    y: 12.0,
                },
            ],
            clusters: vec![0, 1],
        }
    }

    #[test]
    fn the_borrowed_view_carries_the_brush_it_was_asked_for() {
        let owned = owned();
        assert_eq!(owned.as_run(PaintSlot(4)).brush, PaintSlot(4));
        assert_eq!(owned.as_run(PaintSlot(0)).brush, PaintSlot(0));
        assert_eq!(owned.as_run(PaintSlot(0)).glyphs, owned.glyphs.as_slice());
    }

    #[test]
    fn a_cached_run_keys_its_glyphs_exactly_as_a_freshly_shaped_one_does() {
        let owned = owned();
        let fresh = ShapedRun {
            face: owned.face,
            size: owned.size,
            synthetic_bold: owned.synthetic_bold,
            synthetic_slant: owned.synthetic_slant,
            has_color: owned.has_color,
            brush: PaintSlot(0),
            glyphs: &owned.glyphs,
        };
        for glyph in &owned.glyphs {
            assert_eq!(
                owned
                    .as_run(PaintSlot(0))
                    .key_for(*glyph, 3.25, RasterStyle::Subpixel),
                fresh.key_for(*glyph, 3.25, RasterStyle::Subpixel),
            );
        }
    }

    #[test]
    fn a_borrowed_run_copies_into_an_owned_one() {
        let glyphs = [ShapedGlyph {
            glyph: 7,
            x: 1.0,
            y: 2.0,
        }];
        let run = ShapedRun {
            face: FaceId(1),
            size: 12.0,
            synthetic_bold: 0.02,
            synthetic_slant: 8.0,
            has_color: true,
            brush: PaintSlot(2),
            glyphs: &glyphs,
        };
        let owned = ShapedRunOwned::from(run);
        assert_eq!(owned.face, FaceId(1));
        assert_eq!(owned.synthetic_bold, 0.02);
        assert!(owned.has_color);
        assert_eq!(owned.glyphs, glyphs.to_vec());
        assert_eq!(owned.clusters.len(), owned.glyphs.len());
    }
}
