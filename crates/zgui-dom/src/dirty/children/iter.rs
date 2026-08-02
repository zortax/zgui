//! Reading the record back as the children it names.

use crate::arena::store::DocumentStore;
use crate::dirty::children::DirtyChildren;
use crate::dirty::children::repr::{EXACT, Repr};
use crate::id::node_key::{NodeIndex, OptIndex};

impl DirtyChildren {
    /// The children this record names, skipping any that `owner` no longer parents.
    ///
    /// The parent test is what makes a stale entry harmless: a child removed or reparented since
    /// the mark is simply not yielded, so nothing downstream has to check whether a record it was
    /// handed still describes the tree. A child whose slot has been reclaimed outright — a record
    /// held across the end of a frame — is dropped on the same grounds rather than panicking. The
    /// order is unspecified.
    pub fn iter<'doc>(
        &self,
        store: &'doc DocumentStore,
        owner: NodeIndex,
    ) -> impl Iterator<Item = NodeIndex> + 'doc {
        let repr = self.0.get();
        let mut exact = [OptIndex::NONE; EXACT];
        let mut used = 0;
        let mut span = None;
        if repr.len == Repr::SPAN {
            span = repr.slots[0].get();
        } else {
            exact = repr.slots;
            used = repr.len as usize;
        }
        let stop = repr.slots[1].get();

        let mut walked = 0;
        core::iter::from_fn(move || {
            if let Some(current) = span {
                // A slot the arena has since recycled resolves to nothing. The walk cannot be
                // continued through it, so it ends here; the entries beyond it are reached again
                // by whatever re-marks them, and the record is rebuilt on the next unwind.
                let Some(record) = store.try_core(current) else {
                    span = None;
                    return None;
                };
                span = if Some(current) == stop {
                    None
                } else {
                    record.next_sibling()
                };
                return Some(current);
            }
            while walked < used {
                let candidate = exact[walked].get();
                walked += 1;
                if let Some(candidate) = candidate {
                    return Some(candidate);
                }
            }
            None
        })
        // A child the arena has recycled resolves to nothing and is dropped here rather than
        // panicking a walk that was handed a record older than the frame it is reading.
        .filter(move |child| {
            store
                .try_core(*child)
                .is_some_and(|record| record.parent() == Some(owner))
        })
    }
}
