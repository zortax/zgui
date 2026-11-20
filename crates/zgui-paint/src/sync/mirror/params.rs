//! Shader parameter blocks mirrored as parameter slots.

use compositor_types::{ParamsBlock, ParamsId, Transaction};
use rustc_hash::FxHashMap;

use crate::content::shader::{SharedParams, Wake};
use crate::sync::mirror::holds::Holds;

/// The parameter slot of each block an item or a filter drew with.
///
/// A block written by value is found by its bytes, so equal blocks share one slot and items
/// drawing one shader with them can share a draw call. A [`SharedParams`] has one slot, written
/// whenever the block changed since the last frame. A slot is removed once nothing holds it.
#[derive(Default)]
pub(crate) struct Params {
    slots: FxHashMap<Bytes, ParamsId>,
    blocks: FxHashMap<ParamsId, Bytes>,
    shared: FxHashMap<usize, Shared>,
    owners: FxHashMap<ParamsId, usize>,
    wake: Option<Wake>,
}

/// The bytes of a parameter block, which is what equal blocks are found by.
type Bytes = ([u32; 4], [u8; 64]);

/// The slot of one shared block, and the revision last written to it.
struct Shared {
    id: ParamsId,
    params: SharedParams,
    revision: u64,
}

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

    /// Returns the slot of `params`, creating it on first use, and notes it as named by the chunk
    /// being recorded.
    pub(crate) fn shared(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        params: &SharedParams,
    ) -> ParamsId {
        let identity = params.identity();
        if let Some(shared) = self.shared.get(&identity) {
            holds.name(shared.id);
            return shared.id;
        }
        let (block, revision) = params.read();
        let id = tx.create_params(block);
        holds.created(id);
        holds.name(id);
        if let Some(wake) = &self.wake {
            params.wake(wake);
        }
        self.owners.insert(id, identity);
        self.shared.insert(
            identity,
            Shared {
                id,
                params: params.clone(),
                revision,
            },
        );
        id
    }

    /// Writes every shared block that changed since the last call, and returns how many it wrote.
    pub(crate) fn sync(&mut self, tx: &mut Transaction) -> usize {
        let mut written = 0;
        for shared in self.shared.values_mut() {
            let (block, revision) = shared.params.read();
            if revision != shared.revision {
                tx.set_params(shared.id, block);
                shared.revision = revision;
                written += 1;
            }
        }
        written
    }

    /// Makes a change to every shared block call `wake` from now on.
    pub(crate) fn set_wake(&mut self, wake: Wake) {
        for shared in self.shared.values() {
            shared.params.wake(&wake);
        }
        self.wake = Some(wake);
    }

    /// Returns what a change to a shared block wakes.
    pub(crate) fn wake(&self) -> Option<&Wake> {
        self.wake.as_ref()
    }

    /// Forgets the slot `id`, which was removed.
    pub(crate) fn removed(&mut self, id: ParamsId) {
        if let Some(bytes) = self.blocks.remove(&id) {
            self.slots.remove(&bytes);
        }
        if let Some(identity) = self.owners.remove(&id)
            && let Some(shared) = self.shared.remove(&identity)
            && let Some(wake) = &self.wake
        {
            shared.params.forget(wake);
        }
    }
}

impl Drop for Params {
    fn drop(&mut self) {
        if let Some(wake) = &self.wake {
            for shared in self.shared.values() {
                shared.params.forget(wake);
            }
        }
    }
}

impl core::fmt::Debug for Params {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Params")
            .field("slots", &self.slots.len())
            .field("shared", &self.shared.len())
            .finish_non_exhaustive()
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
