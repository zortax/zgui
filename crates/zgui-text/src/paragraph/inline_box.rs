//! Atomic inlines, as the text pipeline sees them.

use zgui_geom::{Css, CssPx, Point};

/// An atomic inline — an image, an `inline-block`, a form control — as one opaque box the line
/// breaker packs between words.
///
/// # Why the alignment shift is an input on every call rather than state
///
/// A shaper places an inline box with its bottom edge on the baseline, so the only lever for moving
/// it is the height it is told. The height handed over is therefore *not* the box's real height: it
/// is the distance from the baseline to the box's top edge after `vertical-align` has been applied,
/// and the real geometry is kept here beside it.
///
/// That has a consequence which is easy to get wrong. Because the shift is baked into a number the
/// shaper already has, re-styling `vertical-align` changes nothing the shaper can notice, and a
/// paragraph whose shaping is cached would not move at all — a silent no-op rather than a visible
/// error. So the shift is resolved by the caller and supplied afresh on every break, and it is part
/// of the breaking key: a shift that moved invalidates the break exactly as a width change would.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlineBoxGeometry {
    /// Correlates this box with the one the caller is laying out.
    pub id: u64,
    /// The byte offset in the generated string the box sits at.
    pub offset: usize,
    /// The real margin-box width.
    pub width: CssPx,
    /// The real margin-box height.
    pub height: CssPx,
    /// Distance from the box's own baseline to its top margin edge, before the shift.
    pub ascent: CssPx,
    /// The resolved `vertical-align` shift, positive upwards.
    pub shift: CssPx,
}

impl InlineBoxGeometry {
    /// The height a shaper is told, which is the distance from the baseline to the shifted top edge.
    ///
    /// ```
    /// use zgui_geom::CssPx;
    /// use zgui_text::InlineBoxGeometry;
    ///
    /// let raised = InlineBoxGeometry {
    ///     id: 1,
    ///     offset: 0,
    ///     width: CssPx(20.0),
    ///     height: CssPx(20.0),
    ///     ascent: CssPx(20.0),
    ///     shift: CssPx(5.0),
    /// };
    /// assert_eq!(raised.shaper_height(), CssPx(25.0));
    /// assert_eq!(raised.below_baseline(), CssPx(-5.0));
    /// ```
    pub fn shaper_height(&self) -> CssPx {
        CssPx(self.ascent.0 + self.shift.0)
    }

    /// How far the box reaches below the baseline once shifted, negative when it clears it.
    pub fn below_baseline(&self) -> CssPx {
        CssPx(self.height.0 - self.shaper_height().0)
    }
}

/// Where one atomic inline ended up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlineBoxPlacement {
    /// Which box.
    pub id: u64,
    /// Its real top-left corner, relative to the top-left of the paragraph.
    pub origin: Point<CssPx, Css>,
    /// Which line it landed on.
    pub line: usize,
}
