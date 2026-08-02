//! One direction-uniform stretch of a shaped line.

use accesskit::TextDirection;
use zgui_geom::CssPx;

use crate::a11y::run::ClusterGeometry;

/// The clusters of one line that run in one direction, in logical order.
///
/// A line is visited as a sequence of these, in the order they are drawn. Inside one of them the
/// clusters are in the order the bytes appear in the string, which is the order an accessibility
/// tree wants and the reverse of the drawn order when the direction is
/// [`TextDirection::RightToLeft`]. Both orders are needed and neither can be recovered from the
/// other without knowing the direction, which is why it travels with the clusters.
///
/// [`start`](ClusterRun::start) is where the run begins measured from the line box's own start
/// edge, in the same coordinate the positioned glyphs of that line use, so a caller that knows
/// where the line landed knows where every cluster landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClusterRun<'a> {
    /// The direction the clusters advance in on the screen.
    pub direction: TextDirection,
    /// Distance from the line box's start edge to the run's leading edge.
    pub start: CssPx,
    /// The clusters, in logical order.
    pub clusters: &'a [ClusterGeometry],
}

impl ClusterRun<'_> {
    /// Whether the clusters are drawn from right to left.
    pub fn is_rtl(&self) -> bool {
        matches!(self.direction, TextDirection::RightToLeft)
    }

    /// The bytes of the generated string this run covers.
    ///
    /// Empty when the run holds no clusters, which is what a line with nothing on it visits.
    ///
    /// ```
    /// use accesskit::TextDirection;
    /// use zgui_geom::CssPx;
    /// use zgui_text::{ClusterGeometry, ClusterRun};
    ///
    /// let clusters = [
    ///     ClusterGeometry { text: 0..1, offset: CssPx(8.0), advance: CssPx(8.0) },
    ///     ClusterGeometry { text: 1..2, offset: CssPx(0.0), advance: CssPx(8.0) },
    /// ];
    /// let run = ClusterRun {
    ///     direction: TextDirection::RightToLeft,
    ///     start: CssPx(0.0),
    ///     clusters: &clusters,
    /// };
    /// assert_eq!(run.text(), 0..2);
    /// assert!(run.is_rtl(), "the second byte is drawn to the left of the first");
    /// ```
    pub fn text(&self) -> core::ops::Range<usize> {
        match (self.clusters.first(), self.clusters.last()) {
            (Some(first), Some(last)) => first.text.start..last.text.end,
            _ => 0..0,
        }
    }

    /// The advance the run occupies.
    pub fn advance(&self) -> CssPx {
        CssPx(
            self.clusters
                .iter()
                .map(|cluster| cluster.advance.0)
                .sum::<f32>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use accesskit::TextDirection;
    use zgui_geom::CssPx;

    use super::ClusterRun;
    use crate::a11y::run::ClusterGeometry;

    #[test]
    fn an_empty_run_reports_an_empty_range_rather_than_panicking() {
        let run = ClusterRun {
            direction: TextDirection::LeftToRight,
            start: CssPx(0.0),
            clusters: &[],
        };
        assert_eq!(run.text(), 0..0);
        assert_eq!(run.advance(), CssPx(0.0));
    }

    #[test]
    fn the_advance_is_the_sum_of_the_clusters_whatever_order_they_sit_in() {
        let clusters = [
            ClusterGeometry {
                text: 0..1,
                offset: CssPx(16.0),
                advance: CssPx(8.0),
            },
            ClusterGeometry {
                text: 1..2,
                offset: CssPx(8.0),
                advance: CssPx(8.0),
            },
            ClusterGeometry {
                text: 2..3,
                offset: CssPx(0.0),
                advance: CssPx(8.0),
            },
        ];
        let run = ClusterRun {
            direction: TextDirection::RightToLeft,
            start: CssPx(4.0),
            clusters: &clusters,
        };
        assert_eq!(run.advance(), CssPx(24.0));
        assert_eq!(run.text(), 0..3);
    }
}
