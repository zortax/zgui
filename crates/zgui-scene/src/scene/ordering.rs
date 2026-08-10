//! Whether this frame's equal-order primitives really are disjoint on the device.
//!
//! The invariant is stated over the rectangles ordering was assigned from, and those are each in
//! the coordinate system its primitive is drawn under. What has to hold for the *consumers* of the
//! invariant is stronger: a batch reordered within an order class is reordered on the device, so it
//! is the device rectangles that must not meet. The two are the same question for a document with
//! no transform in it, and they come apart exactly where a matrix does something to a subtree that
//! ordering never saw — which is what a placement write is.
//!
//! So this resolves each primitive through the coordinate system it names and sweeps the results.
//! Off unless [`invariant::enabled`](crate::invariant::enabled) says otherwise: it is a resolve and
//! a sort per frame, which is the wrong price for a window that is merely running.

use zgui_geom::transformed_bounds;

use crate::order::sweep::{DEFAULT_MAX, Ordered, TooManyAtOneOrder, overlaps};
use crate::prim::PrimitiveKind;
use crate::scene::Scene;

pub use crate::order::sweep::OrderOverlap;

impl Scene {
    /// Every pair of equal-order primitives that cover one another on the device.
    ///
    /// Group markers are left out: a marker is a change of render target rather than something
    /// drawn, and its rectangle is its whole group's, so it meets everything inside it by
    /// construction. Primitives whose order was forced by an explicitly pushed layer are left out
    /// too, because a layer is where a caller takes the sequence back and says so.
    ///
    /// `Err` when one order class is larger than the cap, which is a refusal to answer rather than
    /// an answer of none.
    pub fn order_overlaps(&self) -> Result<Vec<OrderOverlap>, TooManyAtOneOrder> {
        let mut ordered = Vec::with_capacity(self.ops().len());
        for op in self.ops() {
            if matches!(
                op.kind,
                PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd | PrimitiveKind::Vector
            ) {
                continue;
            }
            let Some(order) = self.order_of_op(*op) else {
                continue;
            };
            if self.is_forced_order(order) {
                continue;
            }
            let local = self.ink_of(*op);
            let ink = match self
                .space_of_op(*op)
                .and_then(|space| self.spatial.resolve(space))
            {
                Some(matrix) => transformed_bounds(&matrix, local),
                None => local,
            };
            ordered.push(Ordered {
                order,
                kind: op.kind,
                ink,
            });
        }
        overlaps(&ordered, order_cap())
    }

    /// Panics if two primitives at one draw order cover one another.
    ///
    /// The panic is the point, and so is where it is raised: by the time a renderer has drawn the
    /// frame, the evidence is one glyph behind the box it belongs in, on a surface nobody holds a
    /// reference for.
    ///
    /// # Panics
    ///
    /// If any two primitives share a draw order and a region, or if one order class is too large to
    /// check — the second so that a check which has stopped running says so.
    pub fn check_order_overlap(&self) {
        if !self.is_checking() {
            return;
        }
        match self.order_overlaps() {
            Ok(found) if found.is_empty() => {}
            Ok(found) => panic!(
                "{} pairs of primitives share a draw order and a region: {}",
                found.len(),
                found
                    .iter()
                    .take(8)
                    .map(|overlap| format!(
                        "{:?} over {:?} at order {}",
                        overlap.first, overlap.second, overlap.order
                    ))
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
            Err(too_many) => panic!("{too_many}"),
        }
    }
}

/// How large an order class may be before the check refuses to run.
///
/// Read from `ZGUI_INVARIANTS_ORDER_MAX` once, because it cannot change during a run and reading it
/// per frame would put an environment lookup in the frame loop.
fn order_cap() -> usize {
    use std::sync::OnceLock;
    static CAP: OnceLock<usize> = OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("ZGUI_INVARIANTS_ORDER_MAX")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_MAX)
    })
}
