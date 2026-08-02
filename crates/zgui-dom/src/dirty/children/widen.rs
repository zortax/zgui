//! Recording a mark, and repairing the record when a child leaves.

use crate::arena::store::DocumentStore;
use crate::dirty::children::DirtyChildren;
use crate::dirty::children::place::{Placement, place};
use crate::dirty::children::repr::{EXACT, Repr};
use crate::id::node_key::{NodeIndex, OptIndex};

impl DirtyChildren {
    /// Adds `child` to the marked set, promoting to a span on the fifth distinct entry.
    ///
    /// `owner` is the node whose record this is. A child `owner` does not parent is ignored, which
    /// is what keeps a mark that arrives after the child has been moved away from re-describing the
    /// record in terms of a child list it does not belong to — a run built over the wrong list
    /// reaches none of the children this record was recording, and every obligation it named would
    /// be serviced by nothing.
    ///
    /// Adding a child that is already named changes nothing, which is what keeps repeated marks on
    /// one row from promoting a record that only ever named one child.
    ///
    /// # Panics
    ///
    /// Panics if `child` names no live node of `store`.
    pub fn widen(&self, owner: NodeIndex, child: NodeIndex, store: &DocumentStore) {
        if store.core(child).parent() != Some(owner) {
            return;
        }
        let repr = self.0.get();
        if repr.len == Repr::SPAN {
            self.widen_span(owner, child, store);
            return;
        }
        let used = repr.len as usize;
        if repr.slots[..used].contains(&OptIndex::some(child)) {
            return;
        }
        if used < EXACT {
            let mut slots = repr.slots;
            slots[used] = OptIndex::some(child);
            self.0.set(Repr {
                slots,
                len: repr.len + 1,
            });
            return;
        }
        self.promote(repr, owner, child, store);
    }

    /// Turns an exact list plus one more child into a run covering all five.
    ///
    /// Built by starting from the newest child alone and widening for the four already named, so
    /// there is one description of what growing a run costs and one place it is bounded. Five marks
    /// that sit near one another produce exactly the run between them; five scattered across a wide
    /// list produce the whole child list, which is the same answer the run between them would have
    /// been worth.
    fn promote(&self, repr: Repr, owner: NodeIndex, child: NodeIndex, store: &DocumentStore) {
        self.set_span(child, child);
        for slot in repr.slots {
            if let Some(marked) = slot.get() {
                self.widen(owner, marked, store);
            }
        }
    }

    /// Grows an existing run to cover `child` as well.
    ///
    /// Three tests in order, and each of them is bounded. A run that already reaches from the first
    /// child to the last covers everything and is left alone. Otherwise the bounded scan places
    /// `child` before the run, inside it, or after it, and the run keeps or moves the end on that
    /// side. A child the scan could not place widens the record to the whole child list rather than
    /// paying a walk of it to find out where the child sits.
    fn widen_span(&self, owner: NodeIndex, child: NodeIndex, store: &DocumentStore) {
        let repr = self.0.get();
        let (Some(first), Some(last)) = (repr.slots[0].get(), repr.slots[1].get()) else {
            self.set_span(child, child);
            return;
        };
        if child == first || child == last {
            return;
        }
        let parent = store.core(owner);
        let (head, tail) = (parent.first_child(), parent.last_child());
        if head == Some(first) && tail == Some(last) {
            return;
        }
        match place(store, child, first, last) {
            Placement::Inside => (),
            Placement::Before => self.set_span(child, last),
            Placement::After => self.set_span(first, child),
            Placement::Unplaced => {
                if let (Some(head), Some(tail)) = (head, tail) {
                    self.set_span(head, tail);
                }
            }
        }
    }

    /// Records the inclusive run from `first` to `last`.
    fn set_span(&self, first: NodeIndex, last: NodeIndex) {
        let mut slots = [OptIndex::NONE; EXACT];
        slots[0] = OptIndex::some(first);
        slots[1] = OptIndex::some(last);
        self.0.set(Repr {
            slots,
            len: Repr::SPAN,
        });
    }

    /// Repairs the record for `child`, which is about to be unlinked from the node that owns it.
    ///
    /// Only the span form needs this, and only when `child` is one of the run's two ends. `previous`
    /// and `following` are the child's siblings as they are now, and the end being unlinked
    /// re-anchors onto whichever of them is on its side of the run; a run whose two ends are both
    /// the departing child covered it alone and is forgotten.
    ///
    /// Both ends need it, for reasons that are not the same one.
    ///
    /// Unlinking clears the node's own sibling links, so a run that *started* at the child would
    /// walk out of the child list on its first step and yield nothing at all — every child it
    /// covered unreachable, and the obligations on them serviced by nothing.
    ///
    /// A run that *ended* at the child still yields everything, because the walk that never meets
    /// its end simply runs on to the end of the child list. What it loses is the invariant the rest
    /// of this type rests on: that both ends are live children in the order the run claims. A child
    /// taken out and put back **earlier in the same list** — an ordinary reorder — is then still
    /// named as the run's end, so widening for it takes the "already inside the run" path and
    /// returns, while the run itself no longer reaches it. Its obligations survive with nothing
    /// leading to them.
    ///
    /// The exact form needs no repair either way, because it names children by identity and skips
    /// the ones the owner no longer parents.
    pub fn note_unlinked(
        &self,
        child: NodeIndex,
        previous: Option<NodeIndex>,
        following: Option<NodeIndex>,
    ) {
        let repr = self.0.get();
        if repr.len != Repr::SPAN {
            return;
        }
        let leaving = OptIndex::some(child);
        let (starts, ends) = (repr.slots[0] == leaving, repr.slots[1] == leaving);
        let replacement = match (starts, ends) {
            (false, false) => return,
            (true, true) => None,
            (true, false) => following.map(|following| (0, following)),
            (false, true) => previous.map(|previous| (1, previous)),
        };
        let Some((end, node)) = replacement else {
            self.clear();
            return;
        };
        let mut slots = repr.slots;
        slots[end] = OptIndex::some(node);
        self.0.set(Repr {
            slots,
            len: Repr::SPAN,
        });
    }
}
