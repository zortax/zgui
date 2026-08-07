//! One page of a sparse side table.

/// Default entries per page. 1024 keeps a page of a 16-byte value at 16 KiB.
pub const PAGE_LEN: usize = 1024;

/// A fixed run of [`PAGE_LEN`] entries, allocated in one piece.
pub(crate) struct Page<V, const N: usize>(Box<[V; N]>);

impl<V: Default, const N: usize> Page<V, N> {
    /// A page whose entries are all the default.
    ///
    /// The entries are built straight into the allocation rather than assembled and moved, so a
    /// page costs one allocation whatever it holds.
    pub(crate) fn new() -> Self {
        let mut entries = Vec::new();
        assert!(N > 0, "a sparse-table page must hold at least one entry");
        entries.resize_with(N, V::default);
        match entries.into_boxed_slice().try_into() {
            Ok(entries) => Self(entries),
            Err(_) => unreachable!("the vector was filled to exactly one page"),
        }
    }
}

impl<V, const N: usize> Page<V, N> {
    /// Borrows one entry.
    pub(crate) fn get(&self, slot: usize) -> &V {
        &self.0[slot]
    }

    /// Borrows one entry for modification.
    pub(crate) fn get_mut(&mut self, slot: usize) -> &mut V {
        &mut self.0[slot]
    }

    /// Every entry, in slot order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &V> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{PAGE_LEN, Page};

    #[test]
    fn a_new_page_is_all_default() {
        let page: Page<u32, PAGE_LEN> = Page::new();
        assert!(page.iter().all(|entry| *entry == 0));
        assert_eq!(page.get(PAGE_LEN - 1), &0);
        assert_eq!(page.iter().count(), PAGE_LEN);
    }

    #[test]
    fn a_written_entry_is_the_only_one_that_changes() {
        let mut page: Page<u32, PAGE_LEN> = Page::new();
        *page.get_mut(7) = 1;
        assert_eq!(page.iter().filter(|entry| **entry != 0).count(), 1);
        assert_eq!(page.get(7), &1);
    }
}
