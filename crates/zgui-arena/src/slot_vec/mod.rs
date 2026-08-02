//! The dense side table: one slot per arena slot, for data nearly everything has.

mod entry;

use core::marker::PhantomData;

use crate::key::{ArenaKey, DomainId};

pub use crate::slot_vec::entry::Entry;

/// A side table holding at most one value per arena slot, stored one after another.
///
/// This is the shape to reach for when nearly everything in the arena participates: the table is
/// a single allocation indexed by slot number, so a lookup is a bounds test and a load. When most
/// slots have no value, [`PagedVec`] costs far less.
///
/// Lookups are by slot number and do **not** check the occupancy counter. The counter is checked
/// once, by the arena, where the key is resolved to its value; repeating it per side table would
/// double the cost of every column read for no new information. What makes that sound is the
/// arena's deferred recycling — within a frame, a slot number cannot come to mean a different
/// value — together with the domain check below.
///
/// A table built with [`SlotVec::for_domain`] remembers which arena it belongs to and, in debug
/// builds, rejects a key from any other. A table built with [`SlotVec::new`] is not tied to an
/// arena and does no such check.
///
/// ```
/// use zgui_arena::{ChunkArena, DomainId, SlotVec};
///
/// let mut arena: ChunkArena<&str> = ChunkArena::new(DomainId::FIRST);
/// let key = arena.insert("node");
///
/// let mut depths: SlotVec<_, u32> = SlotVec::for_domain(arena.domain());
/// depths.insert(key, 3);
/// assert_eq!(depths.get(key), Some(&3));
/// assert_eq!(depths.remove(key), Some(3));
/// assert_eq!(depths.get(key), None);
/// ```
///
/// [`PagedVec`]: crate::PagedVec
pub struct SlotVec<K: ArenaKey, V> {
    /// One slot per arena slot, up to the highest one written.
    slots: Vec<Option<V>>,
    /// How many slots hold a value.
    occupied: usize,
    /// The arena this table belongs to, if it was told.
    domain: Option<DomainId>,
    /// The key type this table is indexed by.
    key: PhantomData<fn() -> K>,
}

impl<K: ArenaKey, V> SlotVec<K, V> {
    /// An empty table that is not tied to any arena.
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            occupied: 0,
            domain: None,
            key: PhantomData,
        }
    }

    /// An empty table that belongs to one arena and, in debug builds, rejects keys from others.
    pub const fn for_domain(domain: DomainId) -> Self {
        Self {
            slots: Vec::new(),
            occupied: 0,
            domain: Some(domain),
            key: PhantomData,
        }
    }

    /// How many slots hold a value.
    pub const fn len(&self) -> usize {
        self.occupied
    }

    /// Whether no slot holds a value.
    pub const fn is_empty(&self) -> bool {
        self.occupied == 0
    }

    /// Borrows the value stored for a key.
    pub fn get(&self, key: K) -> Option<&V> {
        self.check(key);
        self.slots.get(key.index() as usize)?.as_ref()
    }

    /// Borrows the value stored for a key, for modification.
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        self.check(key);
        self.slots.get_mut(key.index() as usize)?.as_mut()
    }

    /// Whether a value is stored for a key.
    pub fn contains_key(&self, key: K) -> bool {
        self.get(key).is_some()
    }

    /// Stores a value for a key, returning whatever it replaced.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.entry(key).replace(value)
    }

    /// Removes the value stored for a key and returns it.
    pub fn remove(&mut self, key: K) -> Option<V> {
        self.check(key);
        let previous = self.slots.get_mut(key.index() as usize)?.take();
        if previous.is_some() {
            self.occupied -= 1;
        }
        previous
    }

    /// A view into one slot, occupied or not, that finds the slot only once.
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        self.check(key);
        let index = key.index() as usize;
        if index >= self.slots.len() {
            self.slots.resize_with(index + 1, || None);
        }
        Entry::new(&mut self.slots[index], &mut self.occupied)
    }

    /// Drops every value, keeping the space they occupied.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.occupied = 0;
    }

    /// Every stored value with the slot number it is stored under, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &V)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| Some((index as u32, slot.as_ref()?)))
    }

    /// Every stored value with the slot number it is stored under, for modification.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u32, &mut V)> {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(index, slot)| Some((index as u32, slot.as_mut()?)))
    }

    /// Panics in debug builds if a key from another arena is used on this table.
    #[inline]
    fn check(&self, key: K) {
        debug_assert!(
            self.domain.is_none_or(|domain| domain == key.domain()),
            "a key from another arena was used on a side table"
        );
    }
}

impl<K: ArenaKey, V> Default for SlotVec<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: ArenaKey, V: core::fmt::Debug> core::fmt::Debug for SlotVec<K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::SlotVec;
    use crate::key::{DomainId, Generation, Key};

    fn key(index: u32) -> Key<()> {
        Key::new(index, Generation::FIRST, DomainId::FIRST)
    }

    #[test]
    fn a_missing_slot_reads_as_absent_however_far_past_the_end_it_is() {
        let table: SlotVec<Key<()>, u32> = SlotVec::new();
        assert_eq!(table.get(key(0)), None);
        assert_eq!(table.get(key(1_000_000)), None);
        assert!(table.is_empty());
    }

    #[test]
    fn writing_a_high_slot_leaves_the_ones_below_it_empty() {
        let mut table: SlotVec<Key<()>, u32> = SlotVec::new();
        table.insert(key(4), 9);
        assert_eq!(table.len(), 1);
        assert_eq!(table.get(key(0)), None);
        assert_eq!(table.get(key(4)), Some(&9));
        assert_eq!(table.iter().collect::<Vec<_>>(), vec![(4, &9)]);
    }

    #[test]
    fn insert_reports_what_it_replaced() {
        let mut table: SlotVec<Key<()>, u32> = SlotVec::new();
        assert_eq!(table.insert(key(0), 1), None);
        assert_eq!(table.insert(key(0), 2), Some(1));
        assert_eq!(table.len(), 1);
        assert_eq!(table.remove(key(0)), Some(2));
        assert_eq!(table.remove(key(0)), None);
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn modifying_through_a_borrow_is_visible_afterwards() {
        let mut table: SlotVec<Key<()>, u32> = SlotVec::new();
        table.insert(key(2), 1);
        *table.get_mut(key(2)).expect("stored") += 1;
        for (_, value) in table.iter_mut() {
            *value += 1;
        }
        assert_eq!(table.get(key(2)), Some(&3));
        assert!(table.contains_key(key(2)));
        table.clear();
        assert!(table.is_empty());
    }

    // The check the table promises is a debug-build one, so this is a debug-build test.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "a key from another arena")]
    fn a_bound_table_rejects_a_foreign_key() {
        // Named here rather than beside the module's other imports, because this is the only test
        // that names them and it does not exist in an optimised build.
        use crate::key::{ArenaKind, DocumentId};

        let foreign = DomainId::new(
            DocumentId::new(1).expect("in range"),
            ArenaKind::new(0).expect("in range"),
        );
        let mut table: SlotVec<Key<()>, u32> = SlotVec::for_domain(foreign);
        table.insert(key(0), 1);
    }

    proptest! {
        #![proptest_config(crate::proptest_config::config())]

        /// The table agrees with an ordinary map for every sequence of writes and removals.
        #[test]
        fn it_behaves_like_a_map_keyed_by_slot_number(
            operations in proptest::collection::vec((0_u32..32, any::<Option<u32>>()), 0..200),
        ) {
            let mut table: SlotVec<Key<()>, u32> = SlotVec::new();
            let mut oracle: BTreeMap<u32, u32> = BTreeMap::new();

            for (index, operation) in operations {
                match operation {
                    Some(value) => {
                        prop_assert_eq!(table.insert(key(index), value), oracle.insert(index, value));
                    }
                    None => {
                        prop_assert_eq!(table.remove(key(index)), oracle.remove(&index));
                    }
                }
                prop_assert_eq!(table.len(), oracle.len());
            }

            let stored: Vec<(u32, u32)> = table.iter().map(|(index, value)| (index, *value)).collect();
            let expected: Vec<(u32, u32)> = oracle.into_iter().collect();
            prop_assert_eq!(stored, expected);
        }
    }
}
