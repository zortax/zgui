//! In-place access to one slot of a dense side table.

use core::marker::PhantomData;

use crate::key::ArenaKey;

/// A view into one slot of a [`SlotVec`], occupied or not.
///
/// The point of an entry is to look the slot up once. Reading a slot, deciding it is empty and
/// then writing it walks the table twice; going through an entry walks it once.
///
/// ```
/// use zgui_arena::{ChunkArena, DomainId, SlotVec};
///
/// let mut arena: ChunkArena<&str> = ChunkArena::new(DomainId::FIRST);
/// let key = arena.insert("node");
///
/// let mut widths: SlotVec<_, Vec<u32>> = SlotVec::for_domain(DomainId::FIRST);
/// widths.entry(key).or_default().push(3);
/// widths.entry(key).or_default().push(4);
/// assert_eq!(widths.get(key), Some(&vec![3, 4]));
/// ```
///
/// [`SlotVec`]: crate::SlotVec
pub struct Entry<'a, K: ArenaKey, V> {
    /// The slot itself.
    slot: &'a mut Option<V>,
    /// The table's count of occupied slots, kept right as the entry fills the slot in.
    occupied: &'a mut usize,
    /// The key type this table is indexed by.
    key: PhantomData<fn() -> K>,
}

impl<'a, K: ArenaKey, V> Entry<'a, K, V> {
    /// Wraps a slot and the count it contributes to.
    pub(crate) fn new(slot: &'a mut Option<V>, occupied: &'a mut usize) -> Self {
        Self {
            slot,
            occupied,
            key: PhantomData,
        }
    }

    /// Whether the slot already holds a value.
    pub fn is_occupied(&self) -> bool {
        self.slot.is_some()
    }

    /// Borrows the slot's value, filling it with `value` first if it is empty.
    pub fn or_insert(self, value: V) -> &'a mut V {
        self.or_insert_with(|| value)
    }

    /// Borrows the slot's value, filling it from `make` first if it is empty.
    pub fn or_insert_with(self, make: impl FnOnce() -> V) -> &'a mut V {
        if self.slot.is_none() {
            *self.occupied += 1;
            *self.slot = Some(make());
        }
        self.slot.as_mut().expect("just filled in")
    }

    /// Borrows the slot's value, filling it with the default first if it is empty.
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }

    /// Stores a value in the slot, returning whatever it replaced.
    pub fn replace(self, value: V) -> Option<V> {
        match self.slot.replace(value) {
            Some(previous) => Some(previous),
            None => {
                *self.occupied += 1;
                None
            }
        }
    }

    /// Applies `change` to the value if the slot holds one, and returns the entry either way.
    pub fn and_modify(self, change: impl FnOnce(&mut V)) -> Self {
        if let Some(value) = self.slot.as_mut() {
            change(value);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::key::{DomainId, Generation, Key};
    use crate::slot_vec::SlotVec;

    fn key(index: u32) -> Key<()> {
        Key::new(index, Generation::FIRST, DomainId::FIRST)
    }

    #[test]
    fn an_entry_fills_an_empty_slot_once() {
        let mut table: SlotVec<Key<()>, u32> = SlotVec::new();
        assert!(!table.entry(key(0)).is_occupied());
        assert_eq!(*table.entry(key(0)).or_insert(1), 1);
        assert_eq!(*table.entry(key(0)).or_insert(2), 1);
        assert!(table.entry(key(0)).is_occupied());
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn and_modify_touches_only_an_occupied_slot() {
        let mut table: SlotVec<Key<()>, u32> = SlotVec::new();
        table.entry(key(0)).and_modify(|value| *value += 1);
        assert_eq!(table.get(key(0)), None);

        table.insert(key(0), 1);
        table.entry(key(0)).and_modify(|value| *value += 1);
        assert_eq!(table.get(key(0)), Some(&2));
    }
}
