//! The style engine's bookkeeping word.
//!
//! Four bits, in one atomic, because every one of them is read or written from a worker thread
//! while the traversal runs. They are a single word rather than four cells for the same reason:
//! the word is already paid for, and a bit that the engine only reads today is one release away
//! from being a bit it writes.
//!
//! **What is deliberately *not* here is a descendants-dirty bit.** The engine asks whether
//! anything below an element needs restyling, and the document already stores exactly that, as the
//! subtree half of the node's invalidation word. Keeping a second copy on the engine's schedule
//! would give one obligation two storages retired at two different times, and the one that is
//! retired later is silently dropped by the marking path's own early-out. So the question is
//! answered from the invalidation word, raising it is a subtree mark, and clearing it is a
//! documented no-op.

/// A snapshot of this element's pre-mutation state is recorded and not yet consumed.
pub const HAS_SNAPSHOT: u32 = 1 << 0;

/// The traversal has consumed this element's snapshot.
pub const SNAPSHOT_HANDLED: u32 = 1 << 1;

/// This element's style data has been established at least once, and not cleared since.
///
/// The style data itself is stored inline in every record, so "does this element have data?"
/// cannot be answered by its presence — the naive answer is always yes, which makes the engine's
/// subtree-clearing walk descend into subtrees it should have pruned. This bit is the real answer:
/// establishing data sets it, clearing data clears it *and* resets the inline data in the same
/// call, so the two answers cannot disagree.
pub const STYLED: u32 = 1 << 2;

/// Something below this element has an animation-only restyle pending.
///
/// This one *is* stored, unlike the ordinary descendants-dirty bit, because the engine both raises
/// and clears it inside a single traversal of its own — one storage with one retirement, which is
/// the property that makes a stored flag safe.
pub const ANIMATION_DIRTY_DESCENDANTS: u32 = 1 << 3;

/// Every bit this word defines.
pub const ALL: u32 = HAS_SNAPSHOT | SNAPSHOT_HANDLED | STYLED | ANIMATION_DIRTY_DESCENDANTS;

#[cfg(test)]
mod tests {
    use super::{ALL, ANIMATION_DIRTY_DESCENDANTS, HAS_SNAPSHOT, SNAPSHOT_HANDLED, STYLED};

    #[test]
    fn the_four_bits_are_distinct_and_the_union_is_exactly_them() {
        let bits = [
            HAS_SNAPSHOT,
            SNAPSHOT_HANDLED,
            STYLED,
            ANIMATION_DIRTY_DESCENDANTS,
        ];
        assert_eq!(bits.iter().fold(0, |all, bit| all | bit), ALL);
        assert_eq!(ALL.count_ones(), 4);
    }
}
