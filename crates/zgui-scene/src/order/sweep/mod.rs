//! The check that two primitives at equal draw order really do not overlap.
//!
//! Everything downstream of ordering is allowed to reorder within an order class — sprites are
//! sorted by texture there, so a batch runs until the texture genuinely changes — and that is only
//! sound because two primitives at equal order cannot cover one another. Nothing else in the
//! project can see it go wrong: the pixels are plausible, the transcript is stable, and the failure
//! is one glyph drawn behind the box it belongs in on a document nobody has a reference for.
//!
//! # Why a sweep and not every pair
//!
//! The commonest document is the worst case for the pairwise version. A page of a hundred
//! non-overlapping boxes ends up with a hundred primitives *at order one*, which is the whole point
//! of assigning order by intersection — so the largest order class grows with the document, and at
//! ten thousand controls comparing every pair is about fifty million tests per frame. A check that
//! costs that is a check somebody switches off.
//!
//! A sweep costs `k log k` in the size of the class: sort the rectangles by their left edge, walk
//! them, and keep the ones whose right edge is still ahead of the current left edge. Two
//! rectangles overlap only if they are both in that set at once and their vertical extents meet.

#[cfg(test)]
mod tests;

use zgui_geom::{Device, DevicePx, Rect};

use crate::id::DrawOrder;
use crate::prim::PrimitiveKind;

/// Two primitives given the same draw order that cover one another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderOverlap {
    /// The order both were given.
    pub order: DrawOrder,
    /// The kind of the first.
    pub first: PrimitiveKind,
    /// The kind of the second.
    pub second: PrimitiveKind,
    /// The region they share.
    pub shared: Rect<DevicePx, Device>,
}

/// One primitive, as the sweep sees it.
#[derive(Clone, Copy, Debug)]
pub struct Ordered {
    /// The order it was given.
    pub order: DrawOrder,
    /// What kind of primitive it is.
    pub kind: PrimitiveKind,
    /// What it covers.
    pub ink: Rect<DevicePx, Device>,
}

/// How large an order class may get before the check refuses to run.
///
/// The refusal is loud. A cap that quietly skipped the biggest class would switch the check off
/// exactly where the document is large enough for the failure to be hard to find, which is the
/// shape of a check that passes for years and was never running.
pub const DEFAULT_MAX: usize = 20_000;

/// Every pair of equal-order primitives that overlap, or the class that was too large to check.
///
/// `max` caps the size of one order class. Exceeding it is an error rather than a skip.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_scene::order::sweep::{Ordered, overlaps};
/// use zgui_scene::PrimitiveKind;
///
/// let at = |x: f32| -> Rect<DevicePx, Device> {
///     Rect::new(Point::new(DevicePx(x), DevicePx(0.0)), Size::new(DevicePx(10.0), DevicePx(10.0)))
/// };
/// let one = |x| Ordered { order: 1, kind: PrimitiveKind::Quad, ink: at(x) };
///
/// assert_eq!(overlaps(&[one(0.0), one(40.0)], 16).expect("under the cap"), vec![]);
/// assert_eq!(overlaps(&[one(0.0), one(5.0)], 16).expect("under the cap").len(), 1);
/// ```
pub fn overlaps(
    primitives: &[Ordered],
    max: usize,
) -> Result<Vec<OrderOverlap>, TooManyAtOneOrder> {
    let mut by_order: Vec<&Ordered> = primitives
        .iter()
        .filter(|held| !held.ink.is_empty())
        .collect();
    by_order.sort_by(|one, two| {
        one.order
            .cmp(&two.order)
            .then(left(one.ink).total_cmp(&left(two.ink)))
    });

    let mut found = Vec::new();
    let mut start = 0;
    while start < by_order.len() {
        let order = by_order[start].order;
        let mut end = start;
        while end < by_order.len() && by_order[end].order == order {
            end += 1;
        }
        let class = &by_order[start..end];
        if class.len() > max {
            return Err(TooManyAtOneOrder {
                order,
                held: class.len(),
                max,
            });
        }
        sweep(class, &mut found);
        start = end;
    }
    Ok(found)
}

/// One order class swept, with every overlapping pair appended to `found`.
///
/// The class arrives sorted by left edge, so an active rectangle whose right edge is behind the
/// current left edge can never meet anything later and is dropped.
fn sweep(class: &[&Ordered], found: &mut Vec<OrderOverlap>) {
    let mut active: Vec<&Ordered> = Vec::new();
    for held in class {
        let edge = left(held.ink);
        active.retain(|open: &&Ordered| right(open.ink) > edge);
        for open in &active {
            if let Some(shared) = open.ink.intersection(held.ink)
                && !shared.is_empty()
            {
                found.push(OrderOverlap {
                    order: held.order,
                    first: open.kind,
                    second: held.kind,
                    shared,
                });
            }
        }
        active.push(held);
    }
}

/// An order class larger than the check is willing to sweep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooManyAtOneOrder {
    /// The order that holds too many.
    pub order: DrawOrder,
    /// How many it holds.
    pub held: usize,
    /// The cap it exceeded.
    pub max: usize,
}

impl core::fmt::Display for TooManyAtOneOrder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "draw order {} holds {} primitives, past the cap of {}; raise \
             ZGUI_INVARIANTS_ORDER_MAX to check it or lower the document",
            self.order, self.held, self.max
        )
    }
}

/// A rectangle's left edge.
fn left(rect: Rect<DevicePx, Device>) -> f32 {
    rect.left().0
}

/// A rectangle's right edge.
fn right(rect: Rect<DevicePx, Device>) -> f32 {
    rect.right().0
}
