//! Storage whose element addresses never change.

// The raw-memory operations live in `block`, each stating what its caller must uphold. This
// module is where those obligations are discharged, from the slot bookkeeping in `alloc`.
#![allow(unsafe_code)]

pub(crate) mod alloc;
pub(crate) mod block;
mod recycle;

use crate::chunk::alloc::Slots;
use crate::chunk::block::Block;
use crate::key::{DomainId, Key};

pub use crate::chunk::block::BLOCK_LEN;

/// Storage whose element addresses never change.
///
/// Blocks are allocated individually and are never moved or resized, so a reference handed out by
/// [`ChunkArena::get`] stays valid until the whole arena is dropped. Growth appends a block; it
/// does not reallocate existing blocks. That is the property a growable vector cannot offer and
/// the reason this type exists: a consumer that hands worker threads references into the arena
/// and keeps inserting elsewhere would otherwise be handing out references that a later insertion
/// silently invalidates.
///
/// Removal is deferred, in two steps. [`ChunkArena::remove`] marks a slot dead but leaves the
/// value in place, so a key removed part-way through a frame keeps resolving for the rest of it —
/// which is what lets one pass remove a value while a later pass of the same frame still walks
/// over its key. [`ChunkArena::recycle`], called once per frame after every pass has run, drops
/// those values and offers their slots for reuse. A slot freed during a frame is therefore never
/// handed out again within that frame.
///
/// # Sharing
///
/// An arena is [`Sync`] exactly when what it stores is, and [`Send`] exactly when what it stores
/// is. Safe code can only reach a value through [`ChunkArena::get`], which yields a shared
/// reference, and every mutation takes `&mut self`, so sharing an arena shares its values and
/// nothing more.
///
/// ```
/// use zgui_arena::{ChunkArena, DomainId};
///
/// let mut arena: ChunkArena<String> = ChunkArena::new(DomainId::FIRST);
/// let first = arena.insert("first".to_owned());
/// let second = arena.insert("second".to_owned());
///
/// assert!(arena.remove(first));
/// assert_eq!(arena.get(first).map(String::as_str), Some("first"), "still readable this frame");
///
/// arena.recycle();
/// assert_eq!(arena.get(first), None, "the frame has ended");
/// assert_eq!(arena.get(second).map(String::as_str), Some("second"));
/// ```
pub struct ChunkArena<T> {
    /// The storage. Blocks are appended and never moved.
    blocks: Vec<Box<Block<T>>>,
    /// Which slots hold what, and which are reusable.
    slots: Slots,
    /// The arena this is, so a foreign key can be told apart from one of ours.
    domain: DomainId,
}

impl<T> ChunkArena<T> {
    /// An empty arena that mints keys in `domain`.
    ///
    /// No memory is allocated until the first insertion.
    pub const fn new(domain: DomainId) -> Self {
        Self {
            blocks: Vec::new(),
            slots: Slots::new(),
            domain,
        }
    }

    /// The arena this is: which document owns it, and which of that document's arenas it is.
    pub const fn domain(&self) -> DomainId {
        self.domain
    }

    /// How many values are live.
    ///
    /// A removed value that has not yet been recycled is not live, even though its key still
    /// resolves.
    pub const fn len(&self) -> u32 {
        self.slots.live()
    }

    /// Whether no value is live.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many slots exist, live or not.
    ///
    /// This never decreases. A long-lived arena with heavy churn holds its high-water mark, plus
    /// whatever [`ChunkArena::retired`] has taken out of circulation.
    pub fn capacity(&self) -> u32 {
        self.slots.capacity()
    }

    /// How many slots have been retired because they ran out of occupancy counters.
    ///
    /// A retired slot is never handed out again, so this is the arena's permanent fragmentation.
    /// It grows by one every time a single slot has held 65 535 values, and is worth watching in
    /// an application that churns a long list for hours.
    pub fn retired(&self) -> u32 {
        self.slots.retired()
    }

    /// Stores a value and returns the key that names it.
    ///
    /// The value's address is fixed from here until it is dropped.
    ///
    /// # Panics
    ///
    /// Panics if the arena has run out of slot numbers, which takes more than four billion
    /// simultaneously live values.
    pub fn insert(&mut self, value: T) -> Key<T> {
        self.insert_with(|_| value)
    }

    /// Stores a value built from the key it is about to be stored under, and returns that key.
    ///
    /// This is what a value that holds its own key needs once slots are reused. Predicting the key
    /// instead — "the next slot is the one after the last" — holds only while every value the arena
    /// has ever stored is still there, and stops holding the moment a slot comes back.
    ///
    /// The value's address is fixed from here until it is dropped.
    ///
    /// # Panics
    ///
    /// Panics if the arena has run out of slot numbers, which takes more than four billion
    /// simultaneously live values. A panic from `make` leaves the slot empty rather than claimed.
    pub fn insert_with(&mut self, make: impl FnOnce(Key<T>) -> T) -> Key<T> {
        let (index, generation) = self.slots.allocate();
        let key = Key::new(index, generation, self.domain);
        let value = {
            // A slot claimed for a value that is never written holds nothing to drop, and the
            // arena's own destructor would drop it anyway. Emptying it if `make` unwinds costs one
            // branch on a path nothing else takes, and removes the case.
            let claimed = Claimed {
                slots: &mut self.slots,
                index,
            };
            let value = make(key);
            core::mem::forget(claimed);
            value
        };
        let block = index as usize / BLOCK_LEN;
        if block == self.blocks.len() {
            self.blocks.push(Block::new());
        }
        // SAFETY: the slot was just claimed, so it holds no value and nothing refers to it — a
        // slot only leaves circulation through `recycle` or `take`, both of which empty it first.
        unsafe { self.blocks[block].write(index as usize % BLOCK_LEN, value) };
        key
    }

    /// Borrows the value a key names, if the key still resolves.
    ///
    /// A key resolves from the insertion that issued it until the value is dropped — which is
    /// strictly longer than the value is *live*, because [`ChunkArena::remove`] deliberately
    /// leaves the value in place until the next [`ChunkArena::recycle`]. The check is on the
    /// occupancy counter rather than on liveness, and the counter moves on when the value is
    /// dropped, so a key that resolves is proof the slot still holds what the key was issued for.
    ///
    /// A key is an ordinary value that anything can build, so nothing here rests on a key having
    /// come from this arena: a slot number past the end, a counter no occupant ever carried and a
    /// counter belonging to a slot that is standing empty all resolve to [`None`].
    ///
    /// A key from another arena — another document, or another of this document's arenas — never
    /// resolves here, whatever slot number it carries.
    pub fn get(&self, key: Key<T>) -> Option<&T> {
        if key.domain() != self.domain {
            return None;
        }
        let index = self.slots.resolve(key.index(), key.generation())?;
        // SAFETY: the key resolved, so the slot holds a value, and the slot cannot be emptied
        // while the returned reference lives: every operation that empties a slot takes
        // `&mut self`, which the borrow of `self` behind this reference rules out.
        Some(unsafe { self.blocks[index as usize / BLOCK_LEN].get(index as usize % BLOCK_LEN) })
    }

    /// Borrows a stored value for modification.
    ///
    /// Taking `&mut self` excludes every shared reader, so this cannot race the borrowed reads the
    /// arena's `Sync` promise is stated over. It is the only way to overwrite a field of a stored
    /// value in place; without it a value could be written when it was inserted and never again.
    pub fn get_mut(&mut self, key: Key<T>) -> Option<&mut T> {
        if key.domain() != self.domain {
            return None;
        }
        let index = self.slots.resolve(key.index(), key.generation())?;
        // SAFETY: the key resolved, so the slot holds a value, and `&mut self` is proof that no
        // other reference into the arena is live for as long as the returned one is.
        Some(unsafe { self.blocks[index as usize / BLOCK_LEN].get_mut(index as usize % BLOCK_LEN) })
    }

    /// Whether a key still resolves.
    pub fn contains_key(&self, key: Key<T>) -> bool {
        self.get(key).is_some()
    }

    /// Marks a slot dead.
    ///
    /// The value stays in place and [`ChunkArena::get`] keeps resolving it until
    /// [`ChunkArena::recycle`] drops it and returns the slot to the allocator. Returns whether
    /// the key named a live value; removing an already-removed key changes nothing and returns
    /// `false`.
    pub fn remove(&mut self, key: Key<T>) -> bool {
        if key.domain() != self.domain {
            return false;
        }
        match self.slots.resolve(key.index(), key.generation()) {
            Some(index) => self.slots.remove(index),
            None => false,
        }
    }

    /// Removes a slot and takes its value immediately, forfeiting the resolve-until-recycle
    /// guarantee.
    ///
    /// The key stops resolving the moment this returns, so every reader that could still hold it
    /// must already have run. Where that is not certain, use [`ChunkArena::remove`], which leaves
    /// the value readable until the frame ends. The slot itself is still withheld until the next
    /// [`ChunkArena::recycle`], so its number cannot be reissued within the frame either way.
    pub fn take(&mut self, key: Key<T>) -> Option<T> {
        if key.domain() != self.domain {
            return None;
        }
        let index = self.slots.resolve(key.index(), key.generation())?;
        // SAFETY: the key resolved, so the slot holds a value. `&mut self` rules out any live
        // reference into it, and `vacate` records the slot as empty directly below, so the value
        // is never read again.
        let value =
            unsafe { self.blocks[index as usize / BLOCK_LEN].take(index as usize % BLOCK_LEN) };
        self.slots.vacate(index);
        Some(value)
    }
}

/// A slot claimed for a value that is still being built, emptied again if the build unwinds.
struct Claimed<'slots> {
    /// The bookkeeping the slot was claimed from.
    slots: &'slots mut Slots,
    /// The slot.
    index: u32,
}

impl Drop for Claimed<'_> {
    fn drop(&mut self) {
        self.slots.vacate(self.index);
    }
}

impl<T> core::fmt::Debug for ChunkArena<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ChunkArena")
            .field("domain", &self.domain)
            .field("len", &self.len())
            .field("capacity", &self.capacity())
            .field("retired", &self.retired())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{BLOCK_LEN, ChunkArena};
    use crate::key::{ArenaKind, DocumentId, DomainId, Generation, Key};

    fn other_domain() -> DomainId {
        DomainId::new(
            DocumentId::new(1).expect("in range"),
            ArenaKind::new(2).expect("in range"),
        )
    }

    #[test]
    fn a_value_is_readable_through_the_key_it_was_stored_under() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let key = arena.insert(42_u32);
        assert_eq!(arena.get(key), Some(&42));
        assert_eq!(arena.len(), 1);
        assert!(!arena.is_empty());
    }

    #[test]
    fn addresses_survive_growth_past_a_block_boundary() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let keys: Vec<_> = (0..BLOCK_LEN as u32).map(|n| arena.insert(n)).collect();
        let addresses: Vec<*const u32> = keys
            .iter()
            .map(|key| arena.get(*key).expect("live") as *const u32)
            .collect();

        for n in 0..BLOCK_LEN as u32 * 3 {
            arena.insert(n);
        }

        for (key, address) in keys.iter().zip(addresses) {
            assert_eq!(arena.get(*key).expect("live") as *const u32, address);
        }
    }

    #[test]
    fn a_key_from_another_arena_never_resolves() {
        let mut mine = ChunkArena::new(DomainId::FIRST);
        let mut theirs = ChunkArena::new(other_domain());
        let mine_key = mine.insert(1_u32);
        let theirs_key = theirs.insert(2_u32);
        assert_eq!(mine_key.index(), theirs_key.index());
        assert_eq!(mine_key.generation(), theirs_key.generation());

        assert_eq!(mine.get(theirs_key), None);
        assert_eq!(theirs.get(mine_key), None);
        assert!(!mine.remove(theirs_key));
        assert_eq!(mine.take(theirs_key), None);
    }

    #[test]
    fn a_key_that_was_never_issued_does_not_resolve() {
        let arena: ChunkArena<u32> = ChunkArena::new(DomainId::FIRST);
        let invented = Key::new(0, Generation::FIRST, DomainId::FIRST);
        assert_eq!(arena.get(invented), None);
    }

    #[test]
    fn a_hand_built_key_never_reaches_a_slot_standing_empty() {
        // A slot that has been recycled is already carrying the counter its *next* occupant will
        // be issued, so that counter is guessable — and until the slot is filled again it names
        // nothing at all.
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let key = arena.insert("stored".to_owned());
        arena.remove(key);
        arena.recycle();

        for raw in 1..=8_u16 {
            let guess = Key::new(0, Generation::new(raw).expect("non-zero"), DomainId::FIRST);
            assert_eq!(arena.get(guess), None, "counter {raw}");
            assert!(!arena.remove(guess), "counter {raw}");
            assert_eq!(arena.take(guess), None, "counter {raw}");
        }
    }

    #[test]
    fn take_gives_the_value_back_and_invalidates_the_key_at_once() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let key = arena.insert("owned".to_owned());
        assert_eq!(arena.take(key).as_deref(), Some("owned"));
        assert_eq!(arena.get(key), None);
        assert_eq!(arena.take(key), None);
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn a_slot_taken_from_is_still_withheld_until_the_frame_ends() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let key = arena.insert(1_u32);
        arena.take(key);
        assert_eq!(
            arena.insert(2).index(),
            1,
            "the emptied slot is not reissued"
        );
        arena.recycle();
        let third = arena.insert(3);
        assert_eq!(third.index(), 0);
        assert_eq!(third.generation(), Generation::new(2).expect("non-zero"));
    }

    #[test]
    fn a_value_can_be_built_from_the_key_it_is_stored_under() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        let first = arena.insert_with(|key| key.index());
        arena.remove(first);
        arena.recycle();

        let second = arena.insert_with(|key| key.index());
        assert_eq!(second.index(), first.index(), "the slot came back");
        assert_eq!(
            arena.get(second),
            Some(&second.index()),
            "a predicted key would have named the slot after the last one instead"
        );
    }

    #[test]
    fn a_build_that_panics_leaves_the_slot_empty_rather_than_claimed() {
        let mut arena: ChunkArena<String> = ChunkArena::new(DomainId::FIRST);
        let kept = arena.insert("kept".to_owned());
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            arena.insert_with(|_| panic!("the value could not be built"))
        }));
        assert!(outcome.is_err());

        // The claimed slot holds nothing, so nothing here or in the arena's own destructor may
        // drop it. Both are exercised: the read below, and the drop at the end of the test.
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.get(kept).map(String::as_str), Some("kept"));
    }

    #[test]
    fn the_debug_form_reports_the_arena_without_its_values() {
        let mut arena = ChunkArena::new(DomainId::FIRST);
        arena.insert(1_u32);
        let text = format!("{arena:?}");
        assert!(text.contains("len: 1"), "{text}");
        assert!(text.contains("retired: 0"), "{text}");
    }

    #[test]
    fn an_arena_is_shareable_exactly_when_its_element_is() {
        const fn assert_sync<T: Sync>() {}
        const fn assert_send<T: Send>() {}
        assert_sync::<ChunkArena<u32>>();
        assert_send::<ChunkArena<u32>>();
    }
}
