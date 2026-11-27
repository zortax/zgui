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
/// The fields are [`ShapedRun`]'s, with two differences. The glyphs and the coordinates are owned
/// rather than borrowed. And there is no brush: a slot is a property of the scene the run is drawn into rather than of
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
    /// The normalised variation coordinates the glyphs are drawn at, empty for the default
    /// instance.
    pub coords: Vec<i16>,
    /// How much the glyphs are emboldened to stand in for a weight the face does not have, as a
    /// fraction of the size; zero when the face covers the requested weight.
    pub synthetic_bold: f32,
    /// How far the glyphs are sheared to stand in for an italic the family does not have, in
    /// degrees; zero when a real italic was found.
    pub synthetic_slant: f32,
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
    /// ```
    /// use zgui_space::PaintSlot;
    /// use zgui_text::{FaceId, ShapedGlyph, ShapedRunOwned};
    ///
    /// let owned = ShapedRunOwned {
    ///     face: FaceId(3),
    ///     size: 16.0,
    ///     coords: vec![8192],
    ///     synthetic_bold: 0.0,
    ///     synthetic_slant: 0.0,
    ///     glyphs: vec![ShapedGlyph { glyph: 9, x: 12.25, y: 0.0 }],
    ///     clusters: vec![0],
    /// };
    /// let run = owned.as_run(PaintSlot(2));
    /// assert_eq!(run.brush, PaintSlot(2));
    /// assert_eq!(run.coords, &[8192]);
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
            coords: &self.coords,
            synthetic_bold: self.synthetic_bold,
            synthetic_slant: self.synthetic_slant,
            brush,
            glyphs: &self.glyphs,
            text_start: 0,
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
            coords: run.coords.to_vec(),
            synthetic_bold: run.synthetic_bold,
            synthetic_slant: run.synthetic_slant,
            glyphs: run.glyphs.to_vec(),
            clusters: vec![0; run.glyphs.len()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShapedRunOwned;
    use crate::font::face::FaceId;
    use crate::glyph::run::{ShapedGlyph, ShapedRun};
    use zgui_space::PaintSlot;

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
            coords: &[-4096],
            synthetic_bold: 0.02,
            synthetic_slant: 8.0,
            brush: PaintSlot(2),
            glyphs: &glyphs,
            text_start: 0,
        };
        let owned = ShapedRunOwned::from(run);
        assert_eq!(owned.face, FaceId(1));
        assert_eq!(owned.coords, vec![-4096]);
        assert_eq!(owned.synthetic_bold, 0.02);
        assert_eq!(owned.glyphs, glyphs.to_vec());
        assert_eq!(owned.clusters.len(), owned.glyphs.len());
    }
}
