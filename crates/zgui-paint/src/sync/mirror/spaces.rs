//! Coordinate systems mirrored as transform nodes.

use compositor_types::{Anchor, Matrix4, Transaction, TransformId, TransformNode};
use zgui_scene::SpatialTree;

/// The transform node of each mirrored coordinate system, by slot.
///
/// Each node is a root whose matrix is the slot's whole map onto the device, so scroll offsets and
/// ancestors' transforms reach the compositor as matrix writes.
#[derive(Debug, Default)]
pub(crate) struct Spaces {
    nodes: Vec<Option<Mirrored>>,
    stamp: Option<u64>,
}

/// One mirrored slot.
#[derive(Clone, Copy, Debug)]
struct Mirrored {
    id: TransformId,
    matrix: Matrix4,
}

impl Spaces {
    /// Returns the node of `slot`, creating it on first use.
    ///
    /// A slot that resolves to nothing is mirrored at the identity.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        tree: &SpatialTree,
        slot: u32,
    ) -> TransformId {
        let index = slot as usize;
        if let Some(Some(mirrored)) = self.nodes.get(index) {
            return mirrored.id;
        }
        let matrix = resolve(tree, slot);
        let id = tx.create_transform(TransformNode {
            parent: None,
            local: matrix,
            scroll: None,
            anchor: Anchor::Scrolls,
        });
        if self.nodes.len() <= index {
            self.nodes.resize(index + 1, None);
        }
        self.nodes[index] = Some(Mirrored { id, matrix });
        id
    }

    /// Writes the matrix of every mirrored slot whose map changed since the last call.
    ///
    /// Returns the number of nodes written.
    pub(crate) fn sync(&mut self, tx: &mut Transaction, tree: &SpatialTree) -> usize {
        let stamp = tree.writes();
        if self.stamp == Some(stamp) {
            return 0;
        }
        self.stamp = Some(stamp);
        let mut written = 0;
        for (slot, mirrored) in self.nodes.iter_mut().enumerate() {
            let Some(mirrored) = mirrored else {
                continue;
            };
            let matrix = resolve(tree, slot as u32);
            if matrix != mirrored.matrix {
                tx.set_transform(mirrored.id, matrix);
                mirrored.matrix = matrix;
                written += 1;
            }
        }
        written
    }
}

/// Returns the map of `slot` onto the device, or the identity when it resolves to nothing.
fn resolve(tree: &SpatialTree, slot: u32) -> Matrix4 {
    tree.resolve_at(slot)
        .map_or(Matrix4::IDENTITY, |matrix| Matrix4 {
            columns: matrix.columns,
        })
}
