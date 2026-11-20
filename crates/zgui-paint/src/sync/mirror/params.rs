//! Shader parameter blocks mirrored as parameter slots.

use compositor_types::{ParamsBlock, ParamsId, Transaction};
use rustc_hash::FxHashMap;

use crate::sync::mirror::holds::Holds;

/// The parameter slot of each block an item or a filter drew with, by value.
///
/// Equal blocks share one slot, so items drawing with one shader and equal parameters can share a
/// draw call. A slot is removed once nothing holds it.
#[derive(Debug, Default)]
pub(crate) struct Params {
    slots: FxHashMap<Bytes, ParamsId>,
    blocks: FxHashMap<ParamsId, Bytes>,
}

/// The bytes of a parameter block, which is what equal blocks are found by.
type Bytes = ([u32; 4], [u8; 64]);

impl Params {
    /// Returns the slot holding `block`, creating it on first use, and notes it as named by the
    /// chunk being recorded.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        block: ParamsBlock,
    ) -> ParamsId {
        let id = self.slot(tx, holds, block);
        holds.name(id);
        id
    }

    /// Returns the slot holding `block`, creating it on first use.
    pub(crate) fn slot(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        block: ParamsBlock,
    ) -> ParamsId {
        let bytes = bytes(&block);
        if let Some(id) = self.slots.get(&bytes) {
            return *id;
        }
        let id = tx.create_params(block);
        holds.created(id);
        self.slots.insert(bytes, id);
        self.blocks.insert(id, bytes);
        id
    }

    /// Forgets the slot `id`, which was removed.
    pub(crate) fn removed(&mut self, id: ParamsId) {
        if let Some(bytes) = self.blocks.remove(&id) {
            self.slots.remove(&bytes);
        }
    }
}

/// Returns the bytes of `block`.
fn bytes(block: &ParamsBlock) -> Bytes {
    (
        [
            block.pointer[0].to_bits(),
            block.pointer[1].to_bits(),
            block.hovered.to_bits(),
            block._reserved.to_bits(),
        ],
        block.user,
    )
}
