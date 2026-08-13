//! The sparse side table: pages allocated on first write, for data most things do not have.

mod page;

use core::marker::PhantomData;

use crate::key::{ArenaKey, DomainId};
use crate::paged_vec::page::Page;

pub use crate::paged_vec::page::PAGE_LEN;

/// A sparse side table: a two-level index of [`PAGE_LEN`]-entry pages, each allocated on first
/// write.
///
/// A key whose page is absent reads as the default at the cost of one bounds test and one
/// null-pointer test, so a table most slots do not participate in costs one pointer per
/// [`PAGE_LEN`] slots rather than one value per slot. That difference is the whole reason to
/// choose this over [`SlotVec`]: a dozen dense tables over a hundred thousand slots is tens of
/// megabytes before any of them holds anything, on an index space that only grows.
///
/// Lookups are by slot number and do **not** check the occupancy counter, for the reason given on
/// [`SlotVec`], and a table built with [`PagedVec::for_domain`] rejects keys from other arenas in
/// debug builds the same way.
///
/// ```
/// use zgui_arena::{ChunkArena, DomainId, PagedVec};
///
/// let mut arena: ChunkArena<&str> = ChunkArena::new(DomainId::FIRST);
/// let key = arena.insert("node");
///
/// let mut labels: PagedVec<_, Option<String>> = PagedVec::for_domain(arena.domain());
/// assert_eq!(labels.get(key), None, "nothing has been written, so no page exists");
///
/// *labels.get_mut(key) = Some("hello".to_owned());
/// assert_eq!(labels.get(key), Some(&Some("hello".to_owned())));
/// assert_eq!(labels.pages(), 1);
///
/// labels.clear(key);
/// labels.compact();
/// assert_eq!(labels.pages(), 0);
/// ```
///
/// [`SlotVec`]: crate::SlotVec
pub struct PagedVec<K: ArenaKey, V, const N: usize = PAGE_LEN> {
    /// The page index. A page is absent until something is written into it.
    pages: Vec<Option<Page<V, N>>>,
    /// The pages written to since the last compaction, which are the only ones that can have
    /// become droppable.
    written: Written,
    /// The arena this table belongs to, if it was told.
    domain: Option<DomainId>,
    /// The key type this table is indexed by.
    key: PhantomData<fn() -> K>,
}

/// Which pages have been written to since the last compaction.
///
/// A page holds what it holds until something writes to it, so a page nothing has touched since it
/// was last tested cannot have become droppable in the meantime. Recording the writes therefore
/// turns compaction from a sweep of the whole table into a walk of what one frame touched — which
/// is the difference between a document costing its own size every frame and costing what changed
/// in it.
///
/// A page appears at most once. The flags are what makes that cheap to enforce; the list is what
/// makes the walk proportional to the writes rather than to the table.
#[derive(Debug, Default)]
struct Written {
    /// Whether each page is already in the list.
    flagged: Vec<bool>,
    /// The pages in the list.
    pages: Vec<usize>,
}

impl Written {
    /// Records that one page was written to.
    #[inline]
    fn mark(&mut self, page: usize) {
        if page >= self.flagged.len() {
            self.flagged.resize(page + 1, false);
        }
        if !self.flagged[page] {
            self.flagged[page] = true;
            self.pages.push(page);
        }
    }
}

impl<K: ArenaKey, V, const N: usize> PagedVec<K, V, N> {
    /// An empty table that is not tied to any arena.
    pub const fn new() -> Self {
        Self {
            pages: Vec::new(),
            written: Written {
                flagged: Vec::new(),
                pages: Vec::new(),
            },
            domain: None,
            key: PhantomData,
        }
    }

    /// An empty table that belongs to one arena and, in debug builds, rejects keys from others.
    pub const fn for_domain(domain: DomainId) -> Self {
        Self {
            pages: Vec::new(),
            written: Written {
                flagged: Vec::new(),
                pages: Vec::new(),
            },
            domain: Some(domain),
            key: PhantomData,
        }
    }

    /// How many pages are allocated.
    ///
    /// This is the table's whole cost beyond its index: one page per [`PAGE_LEN`]-slot run that
    /// something has been written into.
    pub fn pages(&self) -> usize {
        self.pages.iter().flatten().count()
    }

    /// Borrows the value stored for a key, or [`None`] if its page has never been written to.
    ///
    /// An absent page reads as the default; this reports the absence instead of materialising it,
    /// so a read never allocates.
    pub fn get(&self, key: K) -> Option<&V> {
        self.check(key);
        let (page, slot) = split::<K, N>(key);
        Some(self.pages.get(page)?.as_ref()?.get(slot))
    }

    /// Every value on an allocated page, with the slot number it is stored under.
    ///
    /// Entries on an allocated page are included even when they hold the default, and entries on
    /// an absent page are not included at all.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &V)> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(index, page)| Some((index, page.as_ref()?)))
            .flat_map(|(index, page)| {
                page.iter()
                    .enumerate()
                    .map(move |(slot, value)| ((index * N + slot) as u32, value))
            })
    }

    /// Drops every page written to since the last compaction on which `is_default` holds for every
    /// entry.
    ///
    /// [`PagedVec::compact`] is this with equality against the default as the test, and is the one
    /// to reach for whenever the value type can be compared. This is for the value types that
    /// cannot — one holding callbacks, or type-erased state — where "nothing worth keeping" has to
    /// be spelled out instead of derived. Dropping a page whose entries are *not* all defaults
    /// silently resets them, so the test has to be the real one.
    ///
    /// Only the pages something has written to are examined, because a page is exactly as full as
    /// it was the last time it was looked at until something writes to it. Every operation that
    /// can put a default into a page records that it did, so this is not an approximation: a page
    /// that has become droppable is always among the ones tested.
    pub fn compact_by(&mut self, is_default: impl Fn(&V) -> bool) {
        let mut dropped = false;
        for index in core::mem::take(&mut self.written.pages) {
            self.written.flagged[index] = false;
            let Some(page) = self.pages.get_mut(index) else {
                continue;
            };
            if page
                .as_ref()
                .is_some_and(|entries| entries.iter().all(&is_default))
            {
                *page = None;
                dropped = true;
            }
        }
        if !dropped {
            return;
        }
        let kept = self.pages.iter().rposition(Option::is_some);
        self.pages.truncate(kept.map_or(0, |index| index + 1));
        self.pages.shrink_to_fit();
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

impl<K: ArenaKey, V: Default, const N: usize> PagedVec<K, V, N> {
    /// Borrows the value stored for a key for modification, allocating its page if needed.
    ///
    /// This is the only operation that allocates, and it allocates at most one page.
    pub fn get_mut(&mut self, key: K) -> &mut V {
        self.check(key);
        let (page, slot) = split::<K, N>(key);
        if page >= self.pages.len() {
            self.pages.resize_with(page + 1, || None);
        }
        // Recorded here rather than only in `clear`, because writing the default through this
        // borrow is indistinguishable from clearing and is how several callers do clear.
        self.written.mark(page);
        self.pages[page].get_or_insert_with(Page::new).get_mut(slot)
    }

    /// Stores a value for a key, returning what was there before.
    ///
    /// A key on an absent page reads as the default and its page is allocated.
    pub fn replace(&mut self, key: K, value: V) -> V {
        core::mem::replace(self.get_mut(key), value)
    }

    /// Borrows many entries at once, exclusively and simultaneously.
    ///
    /// This is what hands a group of workers their disjoint slices of one table: each borrow is
    /// carved off by a progressive split, so the exclusivity is the ordinary borrow rules and no
    /// unsafe code is involved.
    ///
    /// # Panics
    ///
    /// If the keys are not strictly ascending by index — sorted and deduplicated is the caller's
    /// statement that the borrows cannot alias — or if any key's page has never been written.
    pub fn disjoint_mut(&mut self, keys: &[K]) -> Vec<&mut V> {
        assert!(
            keys.windows(2)
                .all(|pair| pair[0].index() < pair[1].index()),
            "disjoint borrows need strictly ascending keys"
        );
        // Bookkeeping first, while the table is still whole: every borrowed entry may be written.
        for &key in keys {
            self.check(key);
            let (page, _) = split::<K, N>(key);
            self.written.mark(page);
        }
        let mut out = Vec::with_capacity(keys.len());
        let mut rest: &mut [Option<Page<V, N>>] = &mut self.pages;
        let mut next_page = 0usize;
        let mut index = 0;
        while index < keys.len() {
            let (page, _) = split::<K, N>(keys[index]);
            let (_, tail) = rest.split_at_mut(page - next_page);
            let (entry_page, tail) = tail
                .split_first_mut()
                .expect("every borrowed key is within the table");
            rest = tail;
            next_page = page + 1;
            let mut slots: &mut [V] = entry_page
                .as_mut()
                .expect("every borrowed key has a written page")
                .slice_mut();
            let mut next_slot = 0usize;
            while index < keys.len() {
                let (entry_page_index, slot) = split::<K, N>(keys[index]);
                if entry_page_index != page {
                    break;
                }
                let (_, tail) = slots.split_at_mut(slot - next_slot);
                let (entry, tail) = tail.split_first_mut().expect("a slot is within its page");
                slots = tail;
                next_slot = slot + 1;
                out.push(entry);
                index += 1;
            }
        }
        out
    }

    /// Resets the value stored for a key to the default.
    ///
    /// A key on an absent page is already the default, so this never allocates.
    pub fn clear(&mut self, key: K) {
        self.check(key);
        let (index, slot) = split::<K, N>(key);
        if let Some(Some(page)) = self.pages.get_mut(index) {
            *page.get_mut(slot) = V::default();
            self.written.mark(index);
        }
    }
}

impl<K: ArenaKey, V: Default + PartialEq, const N: usize> PagedVec<K, V, N> {
    /// Drops every page whose entries are all the default.
    ///
    /// A table that has been emptied returns to costing nothing. Run it once per frame, alongside
    /// the arena's own recycling, so the pages a churning subtree left behind do not accumulate.
    ///
    /// A value type that cannot be compared has [`PagedVec::compact_by`] instead.
    pub fn compact(&mut self) {
        let default = V::default();
        self.compact_by(|entry| *entry == default);
    }
}

/// Splits a key into the page it lives on and its slot within that page.
fn split<K: ArenaKey, const N: usize>(key: K) -> (usize, usize) {
    assert!(N > 0, "a sparse-table page must hold at least one entry");
    let index = key.index() as usize;
    (index / N, index % N)
}

impl<K: ArenaKey, V, const N: usize> Default for PagedVec<K, V, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: ArenaKey, V, const N: usize> core::fmt::Debug for PagedVec<K, V, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PagedVec")
            .field("pages", &self.pages())
            .field("indexed", &self.pages.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{PAGE_LEN, PagedVec};
    use crate::key::{DomainId, Generation, Key};

    fn key(index: u32) -> Key<()> {
        Key::new(index, Generation::FIRST, DomainId::FIRST)
    }

    #[test]
    fn a_table_nothing_has_written_to_since_the_last_compaction_examines_no_page() {
        // The whole of the fix, stated as work: a document at rest ends every frame by compacting
        // its side tables, and a compaction that walked them would cost the document's size per
        // frame for ever. The predicate counts its own calls, so a sweep cannot hide.
        let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
        for page in 0..16u32 {
            *table.get_mut(key(page * PAGE_LEN as u32 + 3)) = 9;
        }
        let examined = core::cell::Cell::new(0usize);
        table.compact_by(|value| {
            examined.set(examined.get() + 1);
            *value == 0
        });
        assert!(
            examined.get() > 0,
            "the pages just written to were examined"
        );
        assert_eq!(table.pages(), 16);

        let untouched = core::cell::Cell::new(0usize);
        table.compact_by(|value| {
            untouched.set(untouched.get() + 1);
            *value == 0
        });
        assert_eq!(
            untouched.get(),
            0,
            "nothing was written, so nothing could have emptied"
        );
        assert_eq!(table.pages(), 16, "and nothing was dropped");
    }

    #[test]
    fn a_page_emptied_through_either_route_is_still_dropped() {
        // Two ways to put the default back: the explicit clear, and a write through `get_mut` of a
        // value that happens to be the default. Both are how callers empty a column, and a
        // compaction that only knew about the first would keep the page for the document's life.
        let mut table: PagedVec<Key<()>, Option<u32>> = PagedVec::new();
        *table.get_mut(key(1)) = Some(1);
        *table.get_mut(key(PAGE_LEN as u32 + 1)) = Some(2);
        assert_eq!(table.pages(), 2);

        table.clear(key(1));
        *table.get_mut(key(PAGE_LEN as u32 + 1)) = None;
        table.compact();
        assert_eq!(
            table.pages(),
            0,
            "both routes leave a page that can be dropped"
        );
    }

    #[test]
    fn a_page_emptied_long_ago_is_dropped_the_first_time_it_is_looked_at() {
        // The record of what was written is not per frame: a table compacted several times over
        // between the write and the emptying must still drop the page. This is what a column
        // holding a value across many frames and then losing it does.
        let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
        *table.get_mut(key(2)) = 5;
        for _ in 0..4 {
            table.compact();
        }
        assert_eq!(table.pages(), 1);
        table.clear(key(2));
        table.compact();
        assert_eq!(table.pages(), 0);
    }

    #[test]
    fn reading_an_absent_key_allocates_nothing() {
        let table: PagedVec<Key<()>, u32> = PagedVec::new();
        assert_eq!(table.get(key(0)), None);
        assert_eq!(table.get(key(u32::MAX)), None);
        assert_eq!(table.pages(), 0);
    }

    #[test]
    fn a_write_allocates_exactly_the_page_it_lands_on() {
        let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
        *table.get_mut(key(3 * PAGE_LEN as u32)) = 7;
        assert_eq!(table.pages(), 1);
        assert_eq!(table.get(key(3 * PAGE_LEN as u32)), Some(&7));
        assert_eq!(
            table.get(key(3 * PAGE_LEN as u32 + 1)),
            Some(&0),
            "a neighbour on the same page reads as the default"
        );
        assert_eq!(
            table.get(key(0)),
            None,
            "a neighbouring page is still absent"
        );
    }

    #[test]
    fn replace_reports_the_previous_value() {
        let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
        assert_eq!(table.replace(key(1), 5), 0);
        assert_eq!(table.replace(key(1), 6), 5);
        table.clear(key(1));
        assert_eq!(table.get(key(1)), Some(&0));
    }

    #[test]
    fn clearing_an_absent_key_allocates_nothing() {
        let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
        table.clear(key(500));
        assert_eq!(table.pages(), 0);
    }

    #[test]
    fn compaction_returns_an_emptied_table_to_nothing() {
        let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
        for page in 0..4 {
            *table.get_mut(key(page * PAGE_LEN as u32 + 5)) = 1;
        }
        assert_eq!(table.pages(), 4);

        table.compact();
        assert_eq!(table.pages(), 4, "pages with a value are kept");

        for page in 0..4 {
            table.clear(key(page * PAGE_LEN as u32 + 5));
        }
        table.compact();
        assert_eq!(table.pages(), 0);
        assert_eq!(table.get(key(5)), None);
    }

    /// A value that cannot be compared, so only [`PagedVec::compact_by`] can compact a table of it.
    #[derive(Default)]
    struct Uncomparable(Vec<u32>);

    #[test]
    fn a_value_that_cannot_be_compared_is_compacted_by_its_own_test() {
        let mut table: PagedVec<Key<()>, Uncomparable> = PagedVec::new();
        table.get_mut(key(0)).0.push(1);
        table.get_mut(key(PAGE_LEN as u32)).0.push(2);
        assert_eq!(table.pages(), 2);

        table.compact_by(|entry| entry.0.is_empty());
        assert_eq!(table.pages(), 2, "neither page is empty");

        table.get_mut(key(PAGE_LEN as u32)).0.clear();
        table.compact_by(|entry| entry.0.is_empty());
        assert_eq!(table.pages(), 1);
        assert_eq!(
            table.get(key(0)).map(|entry| entry.0.as_slice()),
            Some(&[1][..])
        );
        assert!(table.get(key(PAGE_LEN as u32)).is_none());
    }

    #[test]
    fn iteration_covers_the_allocated_pages_in_slot_order() {
        let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
        *table.get_mut(key(PAGE_LEN as u32)) = 3;
        let stored: Vec<(u32, u32)> = table
            .iter()
            .filter(|(_, value)| **value != 0)
            .map(|(index, value)| (index, *value))
            .collect();
        assert_eq!(stored, vec![(PAGE_LEN as u32, 3)]);
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
        let table: PagedVec<Key<()>, u32> = PagedVec::for_domain(foreign);
        let _ = table.get(key(0));
    }

    proptest! {
        #![proptest_config(crate::proptest_config::config())]

        /// One entry per page costs one page per touched page, and compaction gives them back.
        #[test]
        fn a_scattered_table_costs_one_page_per_touched_page(
            slots in proptest::collection::hash_set(0_u32..64, 0..24),
        ) {
            let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
            let keys: Vec<u32> = slots.iter().map(|page| page * PAGE_LEN as u32 + 1).collect();
            for slot in &keys {
                *table.get_mut(key(*slot)) = 1;
            }
            prop_assert_eq!(table.pages(), slots.len());

            for slot in &keys {
                prop_assert_eq!(table.get(key(*slot)), Some(&1));
                prop_assert_eq!(table.get(key(slot + 1)), Some(&0));
            }

            for slot in &keys {
                table.clear(key(*slot));
            }
            table.compact();
            prop_assert_eq!(table.pages(), 0);
        }

        /// A page is dropped only when nothing on it holds a value.
        #[test]
        fn compaction_never_loses_a_value(
            writes in proptest::collection::vec((0_u32..4096, 1_u32..8), 0..64),
        ) {
            let mut table: PagedVec<Key<()>, u32> = PagedVec::new();
            let mut expected = std::collections::BTreeMap::new();
            for (slot, value) in writes {
                *table.get_mut(key(slot)) = value;
                expected.insert(slot, value);
            }
            table.compact();
            for (slot, value) in expected {
                prop_assert_eq!(table.get(key(slot)), Some(&value));
            }
        }
    }
}

#[cfg(test)]
mod disjoint_tests {
    use crate::{ChunkArena, DomainId, PagedVec};

    #[test]
    fn disjoint_borrows_span_pages_and_write_independently() {
        let mut arena: ChunkArena<u32> = ChunkArena::new(DomainId::FIRST);
        let keys: Vec<_> = (0..2100).map(|value| arena.insert(value)).collect();
        let mut table: PagedVec<_, u32> = PagedVec::for_domain(arena.domain());
        for &key in &keys {
            *table.get_mut(key) = 1;
        }
        // Across three pages of the default page length, out of one call.
        let picked = [keys[0], keys[1], keys[1023], keys[1024], keys[2099]];
        let borrows = table.disjoint_mut(&picked);
        assert_eq!(borrows.len(), 5);
        for (offset, entry) in borrows.into_iter().enumerate() {
            *entry += offset as u32;
        }
        assert_eq!(*table.get_mut(picked[4]), 5);
        assert_eq!(*table.get_mut(picked[0]), 1);
        assert_eq!(*table.get_mut(picked[3]), 4);
    }

    #[test]
    #[should_panic(expected = "strictly ascending")]
    fn unsorted_keys_are_refused() {
        let mut arena: ChunkArena<u32> = ChunkArena::new(DomainId::FIRST);
        let first = arena.insert(1);
        let second = arena.insert(2);
        let mut table: PagedVec<_, u32> = PagedVec::for_domain(arena.domain());
        *table.get_mut(first) = 1;
        *table.get_mut(second) = 1;
        let _ = table.disjoint_mut(&[second, first]);
    }
}
