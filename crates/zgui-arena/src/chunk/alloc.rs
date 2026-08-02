//! Slot bookkeeping: what each slot is doing, which counter it is on, and which are reusable.

use crate::key::Generation;

/// What one slot is doing.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum SlotState {
    /// Holds no value. Either it has never held one, or it is waiting to be handed out again.
    Vacant,
    /// Holds a value, and its key resolves.
    Live,
    /// Holds a value that has been removed but not yet dropped. Its key still resolves.
    Removed,
}

/// The per-slot tables and the two free lists.
///
/// Slot numbers are never reused across a frame boundary and never reused after their counter
/// runs out, which is what the two lists and the retirement rule between them enforce.
pub(crate) struct Slots {
    /// The counter each slot is on, or [`None`] once the slot is retired.
    generations: Vec<Option<Generation>>,
    /// What each slot is doing.
    states: Vec<SlotState>,
    /// Slots emptied since the last recycle. Not reusable yet.
    pending: Vec<u32>,
    /// Slots ready to be handed out again.
    free: Vec<u32>,
    /// How many slots are live.
    live: u32,
    /// How many slots have been retired.
    retired: u32,
}

impl Slots {
    /// An empty table.
    pub(crate) const fn new() -> Self {
        Self {
            generations: Vec::new(),
            states: Vec::new(),
            pending: Vec::new(),
            free: Vec::new(),
            live: 0,
            retired: 0,
        }
    }

    /// How many slots hold a live value.
    pub(crate) const fn live(&self) -> u32 {
        self.live
    }

    /// How many slots have been retired because their counters ran out.
    pub(crate) const fn retired(&self) -> u32 {
        self.retired
    }

    /// How many slots exist, live or not.
    pub(crate) fn capacity(&self) -> u32 {
        self.states.len() as u32
    }

    /// What a slot is doing, for a slot number that exists.
    pub(crate) fn state(&self, index: u32) -> SlotState {
        self.states[index as usize]
    }

    /// Claims a slot for a new value, preferring one that has already been recycled.
    ///
    /// # Panics
    ///
    /// Panics if the arena has run out of slot numbers, which takes more than four billion
    /// simultaneously live values.
    pub(crate) fn allocate(&mut self) -> (u32, Generation) {
        self.live += 1;
        if let Some(index) = self.free.pop() {
            self.states[index as usize] = SlotState::Live;
            let generation = self.generations[index as usize]
                .expect("a retired slot never reaches the free list");
            return (index, generation);
        }
        let index = u32::try_from(self.states.len()).expect("arena slot numbers exhausted");
        self.generations.push(Some(Generation::FIRST));
        self.states.push(SlotState::Live);
        (index, Generation::FIRST)
    }

    /// The slot a key names, if the key still resolves.
    ///
    /// A removed slot resolves until its value is dropped, which is the whole of the deferral:
    /// the counter moves on when the value ceases to exist, so a counter that still matches is
    /// proof that the slot holds the value the key was issued for.
    ///
    /// The state is tested as well as the counter, and both tests are load-bearing. A slot that
    /// has been emptied already carries the counter its *next* occupant will be issued, and a
    /// hand-built key naming that counter would otherwise resolve to a slot holding nothing.
    /// Handles are ordinary values that anything can construct, so this cannot be left to
    /// convention.
    pub(crate) fn resolve(&self, index: u32, generation: Generation) -> Option<u32> {
        if self.generations.get(index as usize).copied().flatten()? != generation {
            return None;
        }
        if self.states[index as usize] == SlotState::Vacant {
            return None;
        }
        Some(index)
    }

    /// Marks a live slot as removed, leaving its value in place until the next recycle.
    ///
    /// Returns whether the slot was live; removing an already-removed slot changes nothing.
    pub(crate) fn remove(&mut self, index: u32) -> bool {
        if self.states[index as usize] != SlotState::Live {
            return false;
        }
        self.states[index as usize] = SlotState::Removed;
        self.pending.push(index);
        self.live -= 1;
        true
    }

    /// Marks a slot empty right now, because its value has just been moved out.
    ///
    /// The counter moves on immediately, so the key stops resolving in the same breath as the
    /// value leaves. The slot still waits for a recycle before it can be handed out again.
    pub(crate) fn vacate(&mut self, index: u32) {
        let already_pending = self.states[index as usize] == SlotState::Removed;
        if self.states[index as usize] == SlotState::Live {
            self.live -= 1;
        }
        self.states[index as usize] = SlotState::Vacant;
        self.advance(index);
        if !already_pending {
            self.pending.push(index);
        }
    }

    /// Takes the slots emptied since the last call, leaving the list empty.
    pub(crate) fn take_pending(&mut self) -> Vec<u32> {
        core::mem::take(&mut self.pending)
    }

    /// Puts a recycled list back, so its allocation is reused next frame.
    pub(crate) fn restore_pending(&mut self, mut pending: Vec<u32>) {
        pending.clear();
        if self.pending.is_empty() {
            self.pending = pending;
        }
    }

    /// Records that a removed slot's value has been dropped, and offers the slot for reuse.
    pub(crate) fn release(&mut self, index: u32) {
        debug_assert_eq!(self.states[index as usize], SlotState::Removed);
        self.states[index as usize] = SlotState::Vacant;
        self.advance(index);
        self.offer(index);
    }

    /// Offers an already-vacated slot for reuse.
    pub(crate) fn offer(&mut self, index: u32) {
        if self.generations[index as usize].is_some() {
            self.free.push(index);
        }
    }

    /// Moves a slot on to its next counter, retiring it if it has run out.
    ///
    /// Retirement is permanent. A slot that has held 65 535 values has exhausted the counters
    /// that tell its occupants apart, and handing it out again would let a key from its first
    /// occupant resolve to its 65 536th.
    fn advance(&mut self, index: u32) {
        let generation = &mut self.generations[index as usize];
        let next = generation.and_then(Generation::next);
        if next.is_none() && generation.is_some() {
            self.retired += 1;
        }
        *generation = next;
    }
}

#[cfg(test)]
mod tests {
    use super::{SlotState, Slots};
    use crate::key::Generation;

    #[test]
    fn a_fresh_slot_starts_on_the_first_counter() {
        let mut slots = Slots::new();
        assert_eq!(slots.allocate(), (0, Generation::FIRST));
        assert_eq!(slots.allocate(), (1, Generation::FIRST));
        assert_eq!(slots.live(), 2);
    }

    #[test]
    fn a_removed_slot_still_resolves_until_it_is_released() {
        let mut slots = Slots::new();
        let (index, generation) = slots.allocate();
        assert!(slots.remove(index));
        assert!(!slots.remove(index));
        assert_eq!(slots.resolve(index, generation), Some(index));
        assert_eq!(slots.live(), 0);

        slots.release(index);
        assert_eq!(slots.resolve(index, generation), None);
        assert_eq!(slots.state(index), SlotState::Vacant);
    }

    #[test]
    fn a_vacated_slot_stops_resolving_at_once() {
        let mut slots = Slots::new();
        let (index, generation) = slots.allocate();
        slots.vacate(index);
        assert_eq!(slots.resolve(index, generation), None);
        assert_eq!(slots.take_pending(), vec![index]);
    }

    #[test]
    fn an_empty_slot_resolves_for_no_counter_at_all() {
        let mut slots = Slots::new();
        let (index, generation) = slots.allocate();
        slots.remove(index);
        slots.release(index);

        for raw in 1..=8 {
            let guess = Generation::new(raw).expect("non-zero");
            assert_eq!(
                slots.resolve(index, guess),
                None,
                "an empty slot must resolve for nothing, including the counter it is waiting on"
            );
        }
        assert_ne!(
            slots.resolve(index, generation),
            Some(index),
            "least of all for the counter its last occupant carried"
        );
    }

    #[test]
    fn a_slot_that_runs_out_of_counters_is_retired() {
        let mut slots = Slots::new();
        let (index, _) = slots.allocate();
        for _ in 0..u32::from(u16::MAX) {
            slots.remove(index);
            let pending = slots.take_pending();
            for slot in &pending {
                slots.release(*slot);
            }
            slots.restore_pending(pending);
            if slots.retired() == 1 {
                break;
            }
            let (next, _) = slots.allocate();
            assert_eq!(next, index, "the only free slot is the one just released");
        }
        assert_eq!(slots.retired(), 1);
        assert_eq!(
            slots.allocate().0,
            1,
            "the retired slot is never offered again"
        );
    }
}
