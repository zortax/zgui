//! Which of a node's children owe work.
//!
//! Skipping a clean *subtree* is what the invalidation word does, and it is O(1) per subtree. That
//! is only half the bound a phase walk needs: a node with ten thousand children and one dirty one
//! still probes ten thousand invalidation words unless something narrows the child iteration too.
//! This record is that something.
//!
//! Two decisions in it are worth stating, because both were made the other way first.
//!
//! **Children are recorded by identity, never by position.** Positions among element siblings are
//! numbered lazily, so a structural change can invalidate every position in a child list *after* a
//! mark was recorded and *before* a walk reads it — a walk keyed by position would then descend
//! into the wrong child and the marked one would never be serviced. Identity also costs less:
//! resolving "the five-hundredth child" means following five hundred links, because a child list is
//! a chain and there is no position-to-node map anywhere.
//!
//! **The exact list is the default and the span is the fallback**, not the other way round. The
//! commonest frame there is moves a pointer, which clears a state bit on one row and sets it on
//! another far away; an inclusive span between them covers every clean row in between and pays a
//! probe for each. Four exact entries cover that case, and focus-out/focus-in, and an edge pair,
//! and a single insertion. Only the fifth distinct child promotes to the span, which is where the
//! span genuinely is the cheaper description.
//!
//! **The span runs along the plain child chain and not the element-only one**, and that is a
//! correctness requirement rather than a choice. Text nodes are marked too — editing the text of a
//! node is an obligation like any other — and a span that stepped from element to element could
//! not name one, so the fifth mark on a child list containing text would silently drop whichever
//! marks fell on it. The exact list has no such gap, because it names children by identity, so the
//! two halves of the record would have disagreed with each other about which children exist.
//!
//! **Every step this record takes along a child chain is bounded by a constant**, and that is the
//! difference between a record that costs `O(marks)` over a frame and one that costs
//! `O(marks · width)`. A run says nothing about where a child sits relative to it, so widening for
//! one asks the chain — and a chain is the only structure there is, because positions among
//! siblings are numbered lazily and a number read between a structural change and the renumber that
//! follows it names the wrong child. So the question is asked of at most [`SCAN`] links either side
//! of the child, and when that does not settle it the record widens to *every* child of its owner
//! rather than walking the list to find out. That answer is a superset of the right one, so nothing
//! marked is ever lost; what it costs is probes on children that turned out to be clean, in exactly
//! the case — five or more marks scattered across a wide list — where a run was already going to
//! cover most of them.
//!
//! | Module | Contents |
//! |---|---|
//! | `repr` | the four slots and the tag that says how to read them |
//! | `widen` | recording a mark, and repairing the record when a child leaves |
//! | `place` | placing a child against a run in a bounded number of steps |
//! | `iter` | reading the record back as the children it names |

mod iter;
mod place;
mod repr;
mod widen;

#[cfg(test)]
mod tests;

use core::cell::Cell;

use crate::arena::store::DocumentStore;
use crate::dirty::children::repr::Repr;
use crate::id::node_key::NodeIndex;

pub use crate::dirty::children::place::SCAN;
pub use crate::dirty::children::repr::EXACT;

/// Which of a node's children owe work.
///
/// Widened when a child is marked and rebuilt when a walk unwinds, so a walk descends only into
/// the children that have work rather than into all of them.
///
/// Written between frames, under an exclusive borrow of the document, which is what lets it be a
/// plain cell of copyable data next to fields that need atomics. Reading it during a traversal is
/// a shared read of memory nobody is writing.
#[derive(Debug)]
#[repr(transparent)]
pub struct DirtyChildren(Cell<Repr>);

impl DirtyChildren {
    /// A record naming no children.
    pub const fn empty() -> Self {
        Self(Cell::new(Repr::EMPTY))
    }

    /// Whether this record names no children.
    pub fn is_empty(&self) -> bool {
        let repr = self.0.get();
        repr.len != Repr::SPAN && repr.len == 0
    }

    /// Whether this record has degraded to an inclusive span.
    pub fn is_span(&self) -> bool {
        self.0.get().len == Repr::SPAN
    }

    /// How many children the record names exactly, or [`None`] once it has degraded to a span.
    pub fn exact_len(&self) -> Option<usize> {
        let repr = self.0.get();
        (repr.len != Repr::SPAN).then_some(repr.len as usize)
    }

    /// Forgets every child.
    pub fn clear(&self) {
        self.0.set(Repr::EMPTY);
    }

    /// Replaces the record with exactly `children`, degrading to a span past the fourth.
    ///
    /// This is how a walk rebuilds the record as it unwinds, from the children it found still
    /// owing work. `owner` is the node whose record this is, and children it no longer parents are
    /// dropped rather than recorded — a callback that reparented a child mid-walk leaves exactly
    /// that.
    ///
    /// # Panics
    ///
    /// Panics if any of `children` names no live node of `store`.
    pub fn replace(
        &self,
        owner: NodeIndex,
        children: impl IntoIterator<Item = NodeIndex>,
        store: &DocumentStore,
    ) {
        self.clear();
        for child in children {
            self.widen(owner, child, store);
        }
    }
}

impl Default for DirtyChildren {
    fn default() -> Self {
        Self::empty()
    }
}

// SAFETY: shape 2 — one cell of plain `Copy` data. `Cell::get` is a load and `Cell::set` a store,
// and the record is written only between frames, under an exclusive borrow of the document.
unsafe impl crate::node::discipline::CellDisciplined for DirtyChildren {}
