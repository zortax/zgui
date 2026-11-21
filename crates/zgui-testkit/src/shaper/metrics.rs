//! The fixed face every cluster is measured against.
//!
//! The numbers come from [`FixedMetrics`], which is where the project's one fixed face lives, so a
//! document styled against that metrics source and a paragraph shaped by [`MonoShaper`] agree about
//! how tall an `ex` is. Restating the ratios here would be a second face that looked like the first
//! until one of them was adjusted.
//!
//! At the initial font size of 16 CSS pixels this yields a cluster **8 wide and 16 tall**: an
//! advance of half the size, an ascent of four fifths and a descent of one fifth.
//!
//! [`MonoShaper`]: crate::MonoShaper

use zgui_geom::CssPx;
use zgui_text::StrutMetrics;
use zgui_text::metrics::fixed::{FixedMetrics, ratio};
use zgui_text_style::TextStyle;

/// The advance of one ordinary cluster in `style`, including `letter-spacing`.
///
/// Every character is one cluster of this width, whatever it is: that is the whole of what makes
/// this shaper deterministic without a font file, and it is why a test written against it measures
/// *layout*, never typography.
pub fn advance(style: &TextStyle) -> CssPx {
    CssPx(style.size.0 * ratio::ZERO_ADVANCE + style.letter_spacing.resolve(style.size).0)
}

/// The advance of a space cluster in `style`, which additionally carries `word-spacing`.
///
/// The basis for a percentage is the advance of a space in the chosen face, which here is the
/// ordinary cluster advance.
pub fn space_advance(style: &TextStyle) -> CssPx {
    let ordinary = advance(style);
    CssPx(ordinary.0 + style.word_spacing.resolve(ordinary).0)
}

/// The face's own content area at `size`: ascent above the baseline, descent below.
pub fn content_area(size: CssPx) -> (CssPx, CssPx) {
    (
        FixedMetrics::at(size).ascent,
        FixedMetrics::descent_at(size),
    )
}

/// The strut a block whose root text style is `style` establishes.
pub fn strut(style: &TextStyle) -> StrutMetrics {
    let (ascent, descent) = content_area(style.size);
    StrutMetrics {
        font_ascent: ascent,
        font_descent: descent,
        line_height: style
            .line_height
            .resolve(style.size, CssPx(ascent.0 + descent.0)),
        x_height: FixedMetrics::at(style.size)
            .x_height
            .unwrap_or(CssPx(style.size.0 * ratio::X_HEIGHT)),
        font_size: style.size,
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::CssPx;
    use zgui_text_style::{LengthPercent, TextStyle};

    use super::{advance, space_advance, strut};

    #[test]
    fn a_cluster_is_eight_by_sixteen_at_the_initial_size() {
        let style = TextStyle::initial();
        assert_eq!(style.size, CssPx(16.0));
        assert_eq!(advance(&style), CssPx(8.0));

        let strut = strut(&style);
        assert_eq!(strut.font_ascent, CssPx(12.8));
        assert_eq!(strut.font_descent, CssPx(3.2));
        assert_eq!(strut.line_height, CssPx(16.0));
    }

    #[test]
    fn the_face_is_the_one_the_cascade_resolves_units_against() {
        // `ex` resolves through `FixedMetrics`, and a line box is built from the strut here. Two
        // faces would make `1ex` and half a cluster's advance different numbers.
        use zgui_text::{FaceQuery, FixedMetrics, FontMetricsSource};

        let style = TextStyle::initial();
        let cascade = FixedMetrics::new().face_metrics(&FaceQuery::of(&style), style.size, false);
        assert_eq!(cascade.x_height, Some(strut(&style).x_height));
        assert_eq!(cascade.ascent, strut(&style).font_ascent);
    }

    #[test]
    fn spacing_widens_the_clusters_it_is_asked_to() {
        let mut style = TextStyle::initial();
        style.letter_spacing = LengthPercent::length(CssPx(2.0));
        assert_eq!(advance(&style), CssPx(10.0));

        style.word_spacing = LengthPercent::length(CssPx(4.0));
        assert_eq!(
            advance(&style),
            CssPx(10.0),
            "an ordinary cluster is unaffected"
        );
        assert_eq!(space_advance(&style), CssPx(14.0));
    }

    #[test]
    fn a_larger_size_scales_everything_together() {
        let mut style = TextStyle::initial();
        style.size = CssPx(32.0);
        assert_eq!(advance(&style), CssPx(16.0));
        assert_eq!(strut(&style).line_height, CssPx(32.0));
    }
}
