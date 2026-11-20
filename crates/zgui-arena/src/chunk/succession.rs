//! The counters an arena hands on to the arena that takes its domain over.
//!
//! A domain is a document and one of its arenas, and a process can address only so many documents
//! at once. A document that closes gives its identity back so that a later one can use it. Keys of
//! the closed document can outlive it, so the arena that takes the domain over starts each slot on
//! the counter after the last one the old arena issued for it. A key of the old arena then never
//! resolves in the new one, by the same check that stops a key from resolving after its value is
//! dropped.

use crate::chunk::ChunkArena;
use crate::chunk::alloc::Slots;
use crate::key::{DomainId, Generation};

/// The counter each slot of an arena hands on to its successor.
///
/// ```
/// use zgui_arena::{ChunkArena, DomainId};
///
/// let mut old: ChunkArena<&str> = ChunkArena::new(DomainId::FIRST);
/// let stale = old.insert("closed");
/// let succession = old.succession();
/// drop(old);
///
/// let mut new: ChunkArena<&str> = ChunkArena::succeeding(DomainId::FIRST, &succession);
/// let fresh = new.insert("open");
/// assert_eq!(fresh.index(), stale.index(), "the slot is used again");
/// assert_eq!(new.get(stale), None, "the old key does not resolve");
/// assert_eq!(new.get(fresh), Some(&"open"));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Succession {
    /// The counter each slot's next occupant carries, or [`None`] for a retired slot.
    generations: Box<[Option<Generation>]>,
}

impl Succession {
    /// How many slots the succession covers.
    pub fn len(&self) -> u32 {
        self.generations.len() as u32
    }

    /// Whether the succession covers no slot.
    pub fn is_empty(&self) -> bool {
        self.generations.is_empty()
    }

    /// The counter the next occupant of slot `index` carries.
    ///
    /// [`Generation::FIRST`] for a slot past the end, and [`None`] for a retired slot.
    pub fn next_generation(&self, index: u32) -> Option<Generation> {
        match self.generations.get(index as usize) {
            Some(generation) => *generation,
            None => Some(Generation::FIRST),
        }
    }

    /// How many slots are retired, because they ran out of counters.
    pub fn retired(&self) -> u32 {
        self.generations
            .iter()
            .filter(|slot| slot.is_none())
            .count() as u32
    }
}

impl<T> ChunkArena<T> {
    /// The counters this arena hands on to an arena that takes its domain over.
    ///
    /// Every value the arena holds counts as dropped, so its slot moves on to its next counter.
    pub fn succession(&self) -> Succession {
        Succession {
            generations: self.slots.successors().into_boxed_slice(),
        }
    }

    /// An empty arena in `domain` that takes the domain over from the arena `succession` came
    /// from.
    ///
    /// Each slot starts on the counter the succession gives it, so no key of the old arena
    /// resolves here. Retired slots stay retired. The lowest free slot is handed out first, as in
    /// a new arena.
    pub fn succeeding(domain: DomainId, succession: &Succession) -> Self {
        Self {
            blocks: Vec::new(),
            slots: Slots::succeeding(&succession.generations),
            domain,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::chunk::ChunkArena;
    use crate::key::{DomainId, Generation, Key};

    #[test]
    fn a_slot_of_every_state_hands_on_a_counter_past_its_last_key() {
        let mut old = ChunkArena::new(DomainId::FIRST);
        let live = old.insert(1_u32);
        let recycled = old.insert(2_u32);
        let taken = old.insert(3_u32);
        old.remove(recycled);
        old.recycle();
        let pending = old.insert(4_u32);
        old.remove(pending);
        old.take(taken);
        let succession = old.succession();
        drop(old);

        let mut new = ChunkArena::succeeding(DomainId::FIRST, &succession);
        let keys: Vec<Key<u32>> = (0..4).map(|n| new.insert(n)).collect();
        for stale in [live, recycled, taken, pending] {
            assert_eq!(new.get(stale), None, "{stale:?} resolves in the successor");
        }
        assert_eq!(keys[0].index(), 0, "the lowest slot comes first");
        assert!(keys.iter().all(|key| new.get(*key).is_some()));
    }

    #[test]
    fn a_slot_past_its_last_counter_stays_retired() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let mut key = arena.insert(0_u32);
        while key.generation() != Generation::LAST {
            arena.remove(key);
            arena.recycle();
            key = arena.insert(0);
        }
        let succession = arena.succession();
        assert_eq!(succession.next_generation(0), None);
        assert_eq!(succession.retired(), 1);

        let mut new: ChunkArena<u32> = ChunkArena::succeeding(DomainId::FIRST, &succession);
        assert_eq!(new.retired(), 1);
        assert_eq!(new.insert(1).index(), 1, "the retired slot is skipped");
    }

    #[test]
    fn a_first_slot_past_a_block_of_retired_slots_gets_its_block() {
        use crate::chunk::block::BLOCK_LEN;
        use crate::chunk::succession::Succession;

        let mut generations = vec![None; BLOCK_LEN];
        generations.push(Some(Generation::FIRST));
        let succession = Succession {
            generations: generations.into_boxed_slice(),
        };
        let mut arena: ChunkArena<u32> = ChunkArena::succeeding(DomainId::FIRST, &succession);
        let key = arena.insert(7);
        assert_eq!(key.index() as usize, BLOCK_LEN);
        assert_eq!(arena.get(key), Some(&7));
    }

    #[test]
    fn an_empty_succession_is_a_new_arena() {
        let succession = ChunkArena::<u32>::new(DomainId::FIRST).succession();
        assert!(succession.is_empty());
        assert_eq!(succession.next_generation(7), Some(Generation::FIRST));
        let mut arena: ChunkArena<u32> = ChunkArena::succeeding(DomainId::FIRST, &succession);
        let key = arena.insert(1);
        assert_eq!((key.index(), key.generation()), (0, Generation::FIRST));
    }
}
