//! Reference counts on the compositor ids the mirror created.
//!
//! An id is held by every recorded chunk that names it and by every mirrored node that refers to
//! it, such as a clip node's parent or a transform node's scroll node. An id nothing holds is
//! removed at the next [`Holds::settle`].

use compositor_types::{AnyId, ChunkKey, Transaction};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// The holds on every id the mirror created, and the ids each recorded chunk names.
#[derive(Debug, Default)]
pub(crate) struct Holds {
    counts: FxHashMap<AnyId, u32>,
    held: FxHashMap<AnyId, SmallVec<[AnyId; 2]>>,
    chunks: FxHashMap<ChunkKey, Vec<AnyId>>,
    recording: Vec<AnyId>,
    idle: Vec<AnyId>,
}

impl Holds {
    /// Starts tracking `id`, which nothing holds yet.
    pub(crate) fn created(&mut self, id: impl Into<AnyId>) {
        let id = id.into();
        self.counts.insert(id, 0);
        self.idle.push(id);
    }

    /// Makes `holder` hold `id` for as long as `holder` exists.
    pub(crate) fn hold(&mut self, holder: impl Into<AnyId>, id: impl Into<AnyId>) {
        let id = id.into();
        self.held.entry(holder.into()).or_default().push(id);
        self.acquire(id);
    }

    /// Ends one hold of `holder` on `id`.
    pub(crate) fn unhold(&mut self, holder: impl Into<AnyId>, id: impl Into<AnyId>) {
        let id = id.into();
        if let Some(held) = self.held.get_mut(&holder.into())
            && let Some(at) = held.iter().position(|it| *it == id)
        {
            held.swap_remove(at);
            self.release(id);
        }
    }

    /// Ends every hold of `holder`, which is gone.
    pub(crate) fn forget(&mut self, holder: impl Into<AnyId>) {
        for id in self.held.remove(&holder.into()).unwrap_or_default() {
            self.release(id);
        }
    }

    /// Notes that the chunk being recorded names `id`.
    pub(crate) fn name(&mut self, id: impl Into<AnyId>) {
        self.recording.push(id.into());
    }

    /// Makes the chunk `key` hold the ids named since the last call, in place of what it held.
    pub(crate) fn recorded(&mut self, key: ChunkKey) {
        let mut named = std::mem::take(&mut self.recording);
        named.sort_unstable_by_key(|id| order(*id));
        named.dedup();
        for &id in &named {
            self.acquire(id);
        }
        let old = self.chunks.insert(key, named).unwrap_or_default();
        for id in old {
            self.release(id);
        }
    }

    /// Ends the holds of the chunk `key`, which was removed.
    pub(crate) fn removed(&mut self, key: ChunkKey) {
        for id in self.chunks.remove(&key).unwrap_or_default() {
            self.release(id);
        }
    }

    /// Removes every id nothing holds, and the holds those ids had, and appends them to `out`.
    pub(crate) fn settle(&mut self, tx: &mut Transaction, out: &mut Vec<AnyId>) {
        while let Some(id) = self.idle.pop() {
            if self.counts.get(&id) != Some(&0) {
                continue;
            }
            self.counts.remove(&id);
            tx.remove(id);
            out.push(id);
            for held in self.held.remove(&id).unwrap_or_default() {
                self.release(held);
            }
        }
    }

    fn acquire(&mut self, id: AnyId) {
        if let Some(count) = self.counts.get_mut(&id) {
            *count += 1;
        }
    }

    fn release(&mut self, id: AnyId) {
        if let Some(count) = self.counts.get_mut(&id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.idle.push(id);
            }
        }
    }
}

/// Returns a sort key that orders ids of one kind by slot and generation.
fn order(id: AnyId) -> (u8, u32, u32) {
    match id {
        AnyId::Transform(id) => (0, id.slot(), id.generation()),
        AnyId::Scroll(id) => (1, id.slot(), id.generation()),
        AnyId::Clip(id) => (2, id.slot(), id.generation()),
        AnyId::Effect(id) => (3, id.slot(), id.generation()),
        AnyId::Order(id) => (4, id.slot(), id.generation()),
        AnyId::Paint(id) => (5, id.slot(), id.generation()),
        AnyId::Params(id) => (6, id.slot(), id.generation()),
        AnyId::Image(id) => (7, id.slot(), id.generation()),
        AnyId::Texture(id) => (8, id.slot(), id.generation()),
        AnyId::Shader(id) => (9, id.slot(), id.generation()),
        AnyId::Animation(id) => (10, id.slot(), id.generation()),
    }
}
