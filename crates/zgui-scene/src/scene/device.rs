//! Where each coordinate system puts a rectangle on the device, kept while the tree holds still.
//!
//! A primitive names its coordinate system by slot, and the device answer for a slot is a walk to
//! the root with a matrix product per step. Most primitives of a frame share a few slots, and the
//! tree changes on few frames, so each slot's answer is kept under the tree's
//! [write stamp](crate::PropertyTree::writes) and walked again only after a write.

use zgui_geom::{Device, DevicePx, Matrix4, Rect};

use crate::spatial::{SpatialId, SpatialTree};

/// The device matrix of each slot, under the write stamp it was resolved at.
#[derive(Debug, Default)]
pub(crate) struct DeviceMatrices {
    /// One entry per slot asked for so far.
    slots: Vec<Kept>,
}

/// One slot's kept answer.
#[derive(Clone, Copy, Debug)]
struct Kept {
    /// The tree's stamp when the answer was resolved; zero for no answer, which no tree holds.
    stamp: u64,
    /// What [`SpatialTree::resolve_at`] gave for the slot.
    matrix: Option<Matrix4>,
}

impl Kept {
    /// An entry that holds no answer.
    const NONE: Self = Self {
        stamp: 0,
        matrix: None,
    };
}

impl DeviceMatrices {
    /// The matrix the coordinate system at `slot` maps onto the device.
    ///
    /// `None` for the viewport and for a slot nothing occupies, which place a rectangle where it
    /// is. Otherwise the same answer as [`SpatialTree::resolve_at`].
    pub(crate) fn at(&mut self, tree: &SpatialTree, slot: u32) -> Option<Matrix4> {
        if slot == SpatialId::VIEWPORT.index() {
            return None;
        }
        let index = slot as usize;
        // A slot past the tree's end resolves to nothing. Keep no entry for it, so a stray slot
        // number cannot grow the table.
        if index >= tree.slots().len() {
            return None;
        }
        let stamp = tree.writes();
        if index >= self.slots.len() {
            self.slots.resize(index + 1, Kept::NONE);
        }
        let kept = &mut self.slots[index];
        if kept.stamp != stamp {
            *kept = Kept {
                stamp,
                matrix: resolved(tree, slot),
            };
        }
        debug_assert_eq!(
            kept.matrix.map(bits),
            resolved(tree, slot).map(bits),
            "the kept matrix of slot {slot} is stale at stamp {stamp}",
        );
        kept.matrix
    }
}

/// What [`DeviceMatrices::at`] answers for `slot`, resolved without keeping it.
pub(crate) fn resolved(tree: &SpatialTree, slot: u32) -> Option<Matrix4> {
    if slot == SpatialId::VIEWPORT.index() {
        return None;
    }
    tree.resolve_at(slot)
}

/// A matrix as the bits of its entries, so that a kept NaN compares equal to itself.
fn bits(matrix: Matrix4) -> [[u32; 4]; 4] {
    matrix.columns.map(|column| column.map(f32::to_bits))
}

/// `rect` on the device under `matrix`, or `rect` itself where there is no matrix.
pub(crate) fn place(
    matrix: Option<&Matrix4>,
    rect: Rect<DevicePx, Device>,
) -> Rect<DevicePx, Device> {
    match matrix {
        Some(matrix) => zgui_geom::transformed_bounds(matrix, rect),
        None => rect,
    }
}

#[cfg(test)]
mod tests;
