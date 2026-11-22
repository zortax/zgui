//! The names the boxes' chunks are recorded under.
//!
//! A box's chunks are named after the box. A box that arrives in the frame a box leaves the list
//! it joins takes over the name of the box that left instead. Its chunks then record the departed
//! box's keys again, and its order node is the departed box's node, moved, so the compositor
//! keeps one set of chunks and nodes in place of removing one and creating another.
//!
//! The children of a box that took over a name take over the names of the children of the box
//! that left, in the order that box listed them.

use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::side::BoxKey;

/// The names of the boxes recorded under the name of a box that left.
#[derive(Debug, Default)]
pub(super) struct Names {
    /// The name of each box recorded under another box's name.
    borrowed: FxHashMap<BoxKey, u64>,
    /// The box each borrowed name belongs to.
    holders: FxHashMap<u64, BoxKey>,
    /// The boxes whose chunks were withdrawn this frame, by the box whose list held them, each
    /// with its place in that list.
    departed: FxHashMap<BoxKey, Vec<(u32, BoxKey)>>,
    /// The box each box that arrived this frame took over.
    took_over: FxHashMap<BoxKey, BoxKey>,
    /// The boxes that arrived this frame.
    arrived: FxHashSet<BoxKey>,
}

impl Names {
    /// Returns the name the chunks of the box `key` are recorded under.
    pub(super) fn of(&self, key: BoxKey) -> u64 {
        self.borrowed.get(&key).copied().unwrap_or(key.as_u64())
    }

    /// Returns the box whose chunks are recorded under `name`.
    pub(super) fn box_of(&self, name: u64) -> Option<BoxKey> {
        self.holders
            .get(&name)
            .copied()
            .or_else(|| BoxKey::from_u64(name))
    }

    /// Reports whether the box `key` arrived this frame or holds another box's name.
    pub(super) fn met(&self, key: BoxKey) -> bool {
        self.arrived.contains(&key) || self.borrowed.contains_key(&key)
    }

    /// Notes that the chunks of the box `key`, listed at `place` in the list of `owner`, were
    /// withdrawn this frame.
    pub(super) fn withdrawn(&mut self, key: BoxKey, (owner, place): (BoxKey, u32)) {
        let list = self.departed.entry(owner).or_default();
        if !list.iter().any(|(_, listed)| *listed == key) {
            list.push((place, key));
        }
    }

    /// Notes that the box `key` arrived in the list of `owner`, and gives it the name of the
    /// first box that left that list, if one did.
    ///
    /// The list of a box that took over another box's name is the list the other box held.
    /// Only boxes for which `gone` holds are taken over.
    ///
    /// Returns the box taken over.
    pub(super) fn arrive(
        &mut self,
        key: BoxKey,
        owner: BoxKey,
        gone: impl Fn(BoxKey) -> bool,
    ) -> Option<BoxKey> {
        self.arrived.insert(key);
        let owner = self.took_over.get(&owner).copied().unwrap_or(owner);
        let list = self.departed.get_mut(&owner)?;
        let at = list
            .iter()
            .enumerate()
            .filter(|(_, (_, departed))| gone(*departed))
            .min_by_key(|(_, (place, _))| *place)
            .map(|(at, _)| at)?;
        let (_, departed) = list.swap_remove(at);
        let name = self.borrowed.remove(&departed).unwrap_or(departed.as_u64());
        self.borrowed.insert(key, name);
        self.holders.insert(name, key);
        self.took_over.insert(key, departed);
        Some(departed)
    }

    /// Ends the frame: the departed boxes no box took over and for which `gone` holds give up
    /// the names they held.
    pub(super) fn settle(&mut self, gone: impl Fn(BoxKey) -> bool) {
        for (_, list) in self.departed.drain() {
            for (_, departed) in list {
                if gone(departed)
                    && let Some(name) = self.borrowed.remove(&departed)
                {
                    self.holders.remove(&name);
                }
            }
        }
        self.took_over.clear();
        self.arrived.clear();
    }
}
