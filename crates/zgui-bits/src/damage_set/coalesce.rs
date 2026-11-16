//! The merge that trades extra pixels for fewer redraw passes.

use zgui_geom::{Device, Rect};

use crate::damage_set::merge;

/// The most pixels [`DamageSet::coalesce_dense`](crate::DamageSet::coalesce_dense) adds to a group
/// of rectangles, as a share of the pixels the group covers.
///
/// Each rectangle of a set is one replay of the frame's batch stream. A batch that reaches two
/// rectangles is encoded and shaded once for each, and each pass has its own clear and scissor. A
/// merge removes passes, and the renderer emits and draws the pixels between the rectangles in
/// exchange. This share is the limit on those extra pixels.
///
/// Measured on a discrete GPU at 1280×800, with a full-surface grid of a thousand quads and three
/// thousand discs in each damaged panel: each rectangle less saved about 15 µs of submission and
/// 20 to 25 µs of submission and GPU time. Two panels of 620×380 whose bounds are half again their
/// area still drew faster as one rectangle (152 µs against 161 µs). So two rectangles are enough
/// for a merge.
pub const COALESCE_WASTE_SHARE: f64 = 0.5;

/// A group of held rectangles that merges into its bounds.
pub(super) struct Group<const N: usize> {
    /// Which held rectangles the group takes.
    pub(super) taken: [bool; N],
    /// The rectangle that replaces them.
    pub(super) bounds: Rect<i32, Device>,
    /// The pixels the taken rectangles were absorbed to cover.
    pub(super) covered: i64,
}

/// The group whose merge adds the fewest pixels, when that is within [`COALESCE_WASTE_SHARE`].
///
/// A group starts from a pair of rectangles. Every other rectangle its bounds reach is taken in
/// too, so the set stays disjoint after the merge. `covered` holds the pixels each rectangle was
/// absorbed to cover. After an earlier merge, that is less than the area of the rectangle, so the
/// extra pixels of two merges do not add up past the share. Ties go to the first pair in order.
pub(super) fn cheapest_group<const N: usize>(
    rects: &[Rect<i32, Device>],
    covered: &[i64; N],
) -> Option<Group<N>> {
    let mut best: Option<(i64, Group<N>)> = None;
    for left in 0..rects.len() {
        for right in left + 1..rects.len() {
            let mut taken = [false; N];
            taken[left] = true;
            taken[right] = true;
            let mut bounds = merge::join(rects[left], rects[right]);
            let mut grew = true;
            while grew {
                grew = false;
                for (index, rect) in rects.iter().enumerate() {
                    if !taken[index] && merge::meets(*rect, bounds) {
                        taken[index] = true;
                        bounds = merge::join(bounds, *rect);
                        grew = true;
                    }
                }
            }
            let group_covered = taken
                .iter()
                .zip(covered)
                .filter(|(taken, _)| **taken)
                .map(|(_, covered)| *covered)
                .fold(0i64, i64::saturating_add);
            let extra = merge::area(bounds).saturating_sub(group_covered);
            if extra as f64 > group_covered as f64 * COALESCE_WASTE_SHARE {
                continue;
            }
            if best.as_ref().is_none_or(|(least, _)| extra < *least) {
                let group = Group {
                    taken,
                    bounds,
                    covered: group_covered,
                };
                best = Some((extra, group));
            }
        }
    }
    best.map(|(_, group)| group)
}
