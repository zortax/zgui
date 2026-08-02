//! The engine's bookkeeping bits, and the descent flag that is a view rather than a bit.
//!
//! # Why "something below me needs restyling" is not stored
//!
//! The engine raises a flag saying an element has work below it, descends only where the flag is
//! set, and clears it on a schedule of its own. The document already stores exactly that fact, as
//! the subtree half of every node's invalidation word — so storing the engine's flag as well would
//! give one obligation two storages retired at two different times.
//!
//! That is not merely redundant, it loses marks. Marking a node returns early the moment the node's
//! *own* bits already contain what is being asked for, before any ancestor is touched, so a second
//! mark of a node whose own bits have not yet been retired restores no ancestor flag at all. A flag
//! retired on the engine's schedule, while the invalidation word is retired on the document's,
//! therefore goes missing after the first mark — silently, with no panic and no counter to see it
//! by. The rule that follows holds for every descent flag, not just this one:
//!
//! > A descent flag may only be consumed by the traversal that retires the bits it was raised for.
//!
//! So the question is answered from the invalidation word, raising the flag is a subtree mark, and
//! clearing it is a deliberate no-op: the union is retired exactly once, by the walk that also
//! retires the own bits.
//!
//! The animation-only flag is different and *is* stored, because the engine both raises and clears
//! it inside a single traversal of its own — one storage with one retirement, which is the property
//! that makes a stored flag safe.

use zgui_bits::Dirty;

use crate::node::atomics;
use crate::node::handle::Node;

/// The obligations that mean "the style engine has work at or below this node".
pub const STYLE_WORK: Dirty = Dirty::RESTYLE.union(Dirty::RECASCADE);

impl Node<'_> {
    /// Whether a snapshot of this element's pre-mutation identity is recorded and unconsumed.
    pub fn has_snapshot(self) -> bool {
        self.record().has_atomic(atomics::HAS_SNAPSHOT)
    }

    /// Whether the traversal has already consumed this element's snapshot.
    pub fn handled_snapshot(self) -> bool {
        self.record().has_atomic(atomics::SNAPSHOT_HANDLED)
    }

    /// Records that the traversal has consumed this element's snapshot.
    pub fn set_handled_snapshot(self) {
        self.record().set_atomic(atomics::SNAPSHOT_HANDLED);
    }

    /// Whether anything at or below this node owes the style engine work.
    pub fn has_style_work_below(self) -> bool {
        self.record().has_dirty_descendants(STYLE_WORK)
    }

    /// Records that something below this node owes the style engine work.
    ///
    /// Only the subtree half is written: this node's own obligations are the caller's to state, and
    /// widening them here would restyle an element whose own style nothing asked about.
    pub fn note_style_work_below(self) {
        self.record().dirty().mark_subtree(STYLE_WORK);
    }

    /// Whether an animation-only restyle is pending below this node.
    pub fn has_animation_work_below(self) -> bool {
        self.record()
            .has_atomic(atomics::ANIMATION_DIRTY_DESCENDANTS)
    }

    /// Records that an animation-only restyle is pending below this node.
    pub fn note_animation_work_below(self) {
        self.record()
            .set_atomic(atomics::ANIMATION_DIRTY_DESCENDANTS);
    }

    /// Forgets that an animation-only restyle is pending below this node.
    pub fn clear_animation_work_below(self) {
        self.record()
            .clear_atomic(atomics::ANIMATION_DIRTY_DESCENDANTS);
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn work_below_is_read_from_the_invalidation_word_and_not_from_a_bit() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let child = document.append(root, NodeKind::Element, ElementName::new("item"));

        assert!(!document.node(root).has_style_work_below());
        document.store().core(child).dirty().mark(Dirty::RESTYLE);
        document.node(root).note_style_work_below();
        assert!(document.node(root).has_style_work_below());
        assert!(
            document.node(child).has_style_work_below(),
            "a node's own obligation is part of its own subtree"
        );
    }

    #[test]
    fn a_phase_that_is_not_the_style_engines_does_not_look_like_style_work() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        document.store().core(root).dirty().mark(Dirty::REPAINT);
        assert!(!document.node(root).has_style_work_below());
    }

    #[test]
    fn the_animation_flag_round_trips_because_it_has_one_owner() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let node = document.node(root);
        assert!(!node.has_animation_work_below());
        node.note_animation_work_below();
        assert!(node.has_animation_work_below());
        node.clear_animation_work_below();
        assert!(!node.has_animation_work_below());
    }
}
