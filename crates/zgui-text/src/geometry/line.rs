//! One line box.

use core::ops::Range;

use zgui_geom::CssPx;

/// One laid-out line: where its baseline is, how tall it is, and which text is on it.
///
/// The vertical fields are all relative to the top of the paragraph, so a line's box runs from
/// [`top`](LineGeometry::top) to `top + height` and its baseline sits at
/// [`baseline`](LineGeometry::baseline) — never derived from the font size, because a line holding a
/// tall inline image is taller than its text and a line holding superscripts is taller still.
#[derive(Clone, Debug, PartialEq)]
pub struct LineGeometry {
    /// The byte range of the generated string on this line.
    pub text: Range<usize>,
    /// Distance from the top of the paragraph to the top of the line box.
    pub top: CssPx,
    /// Distance from the top of the paragraph to the line's baseline.
    pub baseline: CssPx,
    /// The height of the line box.
    pub height: CssPx,
    /// The advance the line's content occupies, before alignment.
    pub width: CssPx,
    /// How far the line's content is inset from the paragraph's start edge, after alignment and
    /// any indent.
    pub offset: CssPx,
}

impl LineGeometry {
    /// Distance from the line box's top to its baseline.
    pub fn ascent(&self) -> CssPx {
        CssPx(self.baseline.0 - self.top.0)
    }

    /// Distance from the baseline to the line box's bottom.
    pub fn descent(&self) -> CssPx {
        CssPx(self.top.0 + self.height.0 - self.baseline.0)
    }
}
