//! The strut: the invisible zero-width box every line box is at least as tall as.

use zgui_geom::CssPx;

/// The line-height contribution of the block a paragraph lives in, independent of its content.
///
/// A line box is never shorter than this, which is what stops an empty line, or a line holding only
/// a small inline image, from collapsing to nothing.
///
/// The face's own ascent and descent describe its content area; the difference between the resolved
/// line height and that area is *leading*, which is distributed equally above and below. So the
/// strut's own ascent and descent are the face's plus half the leading each — and the leading is
/// negative when the line height is tighter than the face asks for, which is a legitimate and
/// common thing to write.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StrutMetrics {
    /// The face's ascent at this size.
    pub font_ascent: CssPx,
    /// The face's descent at this size, positive downwards.
    pub font_descent: CssPx,
    /// The resolved `line-height`.
    pub line_height: CssPx,
    /// The face's x-height at this size, which is what `vertical-align: middle` is measured from.
    pub x_height: CssPx,
    /// The font size the three above were taken at, which the script offsets are fractions of.
    pub font_size: CssPx,
}

impl StrutMetrics {
    /// Half the difference between the line height and the face's own content area.
    ///
    /// ```
    /// use zgui_geom::CssPx;
    /// use zgui_text::StrutMetrics;
    ///
    /// let strut = StrutMetrics {
    ///     font_ascent: CssPx(12.0),
    ///     font_descent: CssPx(4.0),
    ///     line_height: CssPx(24.0),
    ///     x_height: CssPx(8.0),
    ///     font_size: CssPx(16.0),
    /// };
    /// assert_eq!(strut.half_leading(), CssPx(4.0));
    /// assert_eq!(strut.ascent(), CssPx(16.0));
    /// assert_eq!(strut.descent(), CssPx(8.0));
    /// ```
    pub fn half_leading(&self) -> CssPx {
        CssPx((self.line_height.0 - (self.font_ascent.0 + self.font_descent.0)) / 2.0)
    }

    /// Distance from the baseline to the top of the strut.
    pub fn ascent(&self) -> CssPx {
        CssPx(self.font_ascent.0 + self.half_leading().0)
    }

    /// Distance from the baseline to the bottom of the strut.
    pub fn descent(&self) -> CssPx {
        CssPx(self.font_descent.0 + self.half_leading().0)
    }
}
