//! Ending a frame: dropping what was removed during it, and offering the slots back.

// Dropping a value where it lies is a raw-memory operation, so its contract comes from `block`
// and is discharged here against the slot bookkeeping in `alloc`.
#![allow(unsafe_code)]

use crate::chunk::ChunkArena;
use crate::chunk::alloc::SlotState;
use crate::chunk::block::BLOCK_LEN;

impl<T> ChunkArena<T> {
    /// Drops this frame's removed values and moves their slots onto the allocation free list.
    ///
    /// Call once per frame, after every pass that can still hold a key from the frame being
    /// ended. Until this runs, a removed value is still readable through its key and its slot
    /// number is still withheld, so no pass of the frame can be handed a key that has silently
    /// come to mean something else since the pass before it.
    ///
    /// Each slot moves on to its next occupancy counter as its value is dropped, so every key
    /// into this frame's removals stops resolving here, all at once.
    pub fn recycle(&mut self) {
        let pending = self.slots.take_pending();
        for index in &pending {
            let index = *index;
            match self.slots.state(index) {
                SlotState::Removed => {
                    // SAFETY: a removed slot holds a value that has not been dropped, and
                    // `&mut self` rules out any live reference into it. `release` records the
                    // slot as empty directly below, so the value is never dropped twice.
                    unsafe {
                        self.blocks[index as usize / BLOCK_LEN]
                            .drop_value(index as usize % BLOCK_LEN);
                    }
                    self.slots.release(index);
                }
                // Emptied by `take`, which already moved the value out and moved the slot on to
                // its next counter. Only the offer is left.
                SlotState::Vacant => self.slots.offer(index),
                SlotState::Live => unreachable!("a slot awaiting recycling is never live"),
            }
        }
        self.slots.restore_pending(pending);
    }
}

impl<T> Drop for ChunkArena<T> {
    fn drop(&mut self) {
        if !core::mem::needs_drop::<T>() {
            return;
        }
        for index in 0..self.slots.capacity() {
            match self.slots.state(index) {
                SlotState::Live | SlotState::Removed => {
                    // SAFETY: both states mean the slot holds a value that has not been dropped.
                    // The arena is being destroyed, so no reference into it can be live, and each
                    // slot number is visited exactly once.
                    unsafe {
                        self.blocks[index as usize / BLOCK_LEN]
                            .drop_value(index as usize % BLOCK_LEN);
                    }
                }
                SlotState::Vacant => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::chunk::ChunkArena;
    use crate::key::{DomainId, Generation};

    /// Counts its own destruction, so a leak and a double drop are both visible.
    struct Tally(Rc<Cell<u32>>);

    impl Drop for Tally {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn a_removed_value_is_dropped_by_the_recycle_and_not_before() {
        let drops = Rc::new(Cell::new(0));
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let key = arena.insert(Tally(Rc::clone(&drops)));

        assert!(arena.remove(key));
        assert_eq!(drops.get(), 0, "still readable, so still alive");
        assert!(arena.get(key).is_some());

        arena.recycle();
        assert_eq!(drops.get(), 1);
        assert!(arena.get(key).is_none());
    }

    #[test]
    fn a_recycled_slot_is_reissued_on_the_next_counter() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let first = arena.insert(1_u32);
        arena.remove(first);
        arena.recycle();

        let second = arena.insert(2_u32);
        assert_eq!(second.index(), first.index());
        assert_ne!(second.generation(), first.generation());
        assert_eq!(second.generation(), Generation::new(2).expect("non-zero"));
        assert_eq!(
            arena.get(first),
            None,
            "the old key does not follow the slot"
        );
        assert_eq!(arena.get(second), Some(&2));
    }

    #[test]
    fn a_slot_freed_this_frame_is_not_reissued_this_frame() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let first = arena.insert(1_u32);
        arena.remove(first);
        let second = arena.insert(2_u32);
        assert_ne!(second.index(), first.index());
        assert_eq!(arena.get(first), Some(&1), "removed but not yet recycled");
    }

    #[test]
    fn recycling_an_untouched_arena_does_nothing() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let key = arena.insert(1_u32);
        arena.recycle();
        arena.recycle();
        assert_eq!(arena.get(key), Some(&1));
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn dropping_the_arena_drops_live_and_removed_values_exactly_once() {
        let drops = Rc::new(Cell::new(0));
        {
            let mut arena = ChunkArena::new(DomainId::FIRST);
            for _ in 0..3 {
                arena.insert(Tally(Rc::clone(&drops)));
            }

            let recycled = arena.insert(Tally(Rc::clone(&drops)));
            arena.remove(recycled);
            arena.recycle();
            assert_eq!(drops.get(), 1, "only the recycled one so far");

            let removed = arena.insert(Tally(Rc::clone(&drops)));
            arena.remove(removed);
            assert_eq!(drops.get(), 1, "this frame has not ended");
        }
        assert_eq!(drops.get(), 5);
    }

    #[test]
    fn taking_a_value_out_leaves_nothing_for_the_recycle_to_drop() {
        let drops = Rc::new(Cell::new(0));
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let key = arena.insert(Tally(Rc::clone(&drops)));
        let value = arena.take(key).expect("live");
        arena.recycle();
        assert_eq!(drops.get(), 0, "the caller owns it now");
        drop(value);
        assert_eq!(drops.get(), 1);
    }
}
