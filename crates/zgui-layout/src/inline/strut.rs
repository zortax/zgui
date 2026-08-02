//! The invisible box every line is at least as tall as, and what each run adds to it.
//!
//! CSS 2.1, in its line-height chapter, builds a line box from the *strut* — a zero-width box
//! carrying the establishing
//! block's own font and line height — plus every inline-level box on the line, each contributing an
//! extent above and an extent below the baseline. A run's contribution is its face's ascent and
//! descent plus half of its own leading on each side, and the leading is its own `line-height` less
//! its own content area, so two runs in different sizes contribute two different pairs and the
//! taller wins on each side independently.
//!
//! That is why the extents are computed from the runs rather than read off a shaper's line: a
//! shaper takes one leading for the whole line, which agrees with CSS only when every run on it has
//! the same line height.

use zgui_text::{StrutMetrics, StyledRun};

use crate::inline::vertical_align::scale_strut;
use crate::measure::MeasureContent;

/// The extents one line reaches either side of its baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extents {
    /// How far the line reaches above its baseline.
    pub above: f32,
    /// How far it reaches below.
    pub below: f32,
}

impl Extents {
    /// The extents of a strut on its own.
    pub fn of(strut: &StrutMetrics) -> Self {
        Self {
            above: strut.ascent().0,
            below: strut.descent().0,
        }
    }

    /// The pair that covers both of two contributions.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            above: self.above.max(other.above),
            below: self.below.max(other.below),
        }
    }

    /// The line box's height.
    pub fn height(self) -> f32 {
        self.above + self.below
    }
}

/// The strut each run contributes, in the units layout works in.
///
/// One entry per run, in the runs' own order, so the answer for a line is a walk over the runs that
/// overlap it. Measuring is asked of the content rather than derived from the style, because the
/// numbers are the face's; two runs in one style cost one measurement, since the measurer holds the
/// answers against the style they came from.
pub fn of_runs<C: MeasureContent>(content: &mut C, runs: &[StyledRun], scale: f32) -> Vec<Extents> {
    runs.iter()
        .map(|run| Extents::of(&scale_strut(content.strut(&run.style), scale)))
        .collect()
}

#[cfg(test)]
mod tests {
    use zgui_geom::CssPx;
    use zgui_text::StrutMetrics;

    use super::Extents;

    #[test]
    fn a_struts_extents_include_half_its_leading_on_each_side() {
        // A 16 px face with a 12/4 content area asked to sit in a 24 px line: four pixels of
        // leading go above and four below.
        let strut = StrutMetrics {
            font_ascent: CssPx(12.0),
            font_descent: CssPx(4.0),
            line_height: CssPx(24.0),
            x_height: CssPx(8.0),
            font_size: CssPx(16.0),
        };
        let extents = Extents::of(&strut);
        assert_eq!(extents.above, 16.0);
        assert_eq!(extents.below, 8.0);
        assert_eq!(extents.height(), 24.0);
    }

    #[test]
    fn a_negative_leading_pulls_both_sides_in_rather_than_one() {
        // A line height tighter than the face asks for is legitimate and common.
        let strut = StrutMetrics {
            font_ascent: CssPx(12.0),
            font_descent: CssPx(4.0),
            line_height: CssPx(12.0),
            x_height: CssPx(8.0),
            font_size: CssPx(16.0),
        };
        let extents = Extents::of(&strut);
        assert_eq!(extents.above, 10.0);
        assert_eq!(extents.below, 2.0);
    }

    #[test]
    fn the_union_takes_the_greater_of_each_side_independently() {
        let tall_above = Extents {
            above: 20.0,
            below: 2.0,
        };
        let deep_below = Extents {
            above: 8.0,
            below: 9.0,
        };
        assert_eq!(
            tall_above.union(deep_below),
            Extents {
                above: 20.0,
                below: 9.0
            }
        );
    }
}
