//! The filled rectangles a selected range is drawn as.

use zgui_geom::{Css, CssPx, Point, Size};

/// One filled rectangle of a selection, in the paragraph's own coordinates.
///
/// A range is one band per line in the simple case and more than one on a line holding two
/// directions: a logically contiguous range is visually split there, and painting it as a single
/// rectangle from its first cluster to its last would cover text that is not selected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    /// Which line it is on.
    pub line: usize,
    /// The top-left corner, measured from the paragraph's top-left corner.
    pub origin: Point<CssPx, Css>,
    /// How wide and how tall.
    pub size: Size<CssPx, Css>,
}

impl Band {
    /// The right-hand edge.
    pub fn right(&self) -> CssPx {
        CssPx(self.origin.x.0 + self.size.width.0)
    }
}

/// Merges the x-intervals of one line into as few as cover them.
///
/// The intervals arrive in the order the clusters were visited, which on a right-to-left run
/// descends, so they are sorted before being merged. Two intervals that merely touch are one band:
/// a hairline of unpainted background between two adjacent selected letters is visible, and on a
/// long selection it is visible once per letter.
pub(crate) fn merge(mut intervals: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut merged: Vec<(f32, f32)> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        match merged.last_mut() {
            Some(held) if start <= held.1 + f32::EPSILON => held.1 = held.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::merge;

    #[test]
    fn touching_intervals_become_one_band_and_disjoint_ones_stay_two() {
        let merged = merge(vec![(8.0, 16.0), (0.0, 8.0), (40.0, 48.0)]);
        assert_eq!(
            merged,
            vec![(0.0, 16.0), (40.0, 48.0)],
            "two adjacent letters must not be painted with a gap between them"
        );
    }

    #[test]
    fn intervals_arriving_in_descending_order_are_still_merged() {
        // Which is what a right-to-left run hands over: its clusters are visited in logical order
        // and their positions descend.
        let merged = merge(vec![(16.0, 24.0), (8.0, 16.0), (0.0, 8.0)]);
        assert_eq!(merged, vec![(0.0, 24.0)]);
    }
}
