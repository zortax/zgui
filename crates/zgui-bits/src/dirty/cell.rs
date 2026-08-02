//! One atomic word holding a node's own obligations beside its subtree's union.

use core::fmt;

use crate::dirty::bits::Dirty;
use crate::sync::{AtomicU64, Ordering};

/// How far the subtree union sits above the node's own bits inside the word.
const SUBTREE_SHIFT: u32 = 32;

/// Splits a packed word into the own half and the subtree half.
const fn unpack(word: u64) -> (Dirty, Dirty) {
    (
        Dirty::from_bits_truncate(word as u32),
        Dirty::from_bits_truncate((word >> SUBTREE_SHIFT) as u32),
    )
}

/// Packs a set of bits into both halves of a word.
const fn both(bits: Dirty) -> u64 {
    let raw = bits.bits() as u64;
    raw | (raw << SUBTREE_SHIFT)
}

/// Packs a set of bits into the subtree half of a word.
const fn subtree_half(bits: Dirty) -> u64 {
    (bits.bits() as u64) << SUBTREE_SHIFT
}

/// One node's own and subtree obligations, packed for a single atomic update.
///
/// The low 32 bits are what this node owes; the high 32 are the union of what everything at or
/// below it owes. Storing both in one word means marking a node and propagating to its ancestors
/// is one atomic read-modify-write per level, and every phase walk can skip a clean subtree by
/// testing one word.
///
/// The two halves never interfere: no operation on the own bits can set or clear a subtree bit,
/// and no operation on the subtree union can touch the own bits.
///
// The example builds a cell, whose atomic is the model checker's under the `loom` feature and
// panics unless it is built inside `loom::model`. The example is still compiled there, only
// not run.
#[cfg_attr(not(feature = "loom"), doc = "```")]
#[cfg_attr(feature = "loom", doc = "```no_run")]
/// use zgui_bits::{Dirty, DirtyCell};
///
/// let cell = DirtyCell::clean();
/// assert!(cell.mark(Dirty::REPAINT));      // the union gained a bit: tell the parent
/// assert!(!cell.mark(Dirty::REPAINT));     // it already had it: the walk stops here
/// assert_eq!(cell.own(), Dirty::REPAINT);
/// assert_eq!(cell.subtree(), Dirty::REPAINT);
///
/// cell.clear_own(Dirty::REPAINT);
/// assert_eq!(cell.own(), Dirty::empty());
/// assert_eq!(cell.subtree(), Dirty::REPAINT);
/// ```
///
/// Every method takes `&self`, so a cell can be marked from a worker thread while a walk reads it.
#[repr(transparent)]
pub struct DirtyCell(AtomicU64);

impl DirtyCell {
    /// A cell owing nothing, at or below itself.
    pub fn clean() -> Self {
        Self(AtomicU64::new(0))
    }

    /// A cell owing `own` itself and `subtree` across everything at or below it.
    ///
    /// The two are stored exactly as given; `own` is not folded into `subtree`.
    pub fn new(own: Dirty, subtree: Dirty) -> Self {
        Self(AtomicU64::new(own.bits() as u64 | subtree_half(subtree)))
    }

    /// What this node owes.
    pub fn own(&self) -> Dirty {
        unpack(self.0.load(Ordering::Acquire)).0
    }

    /// The union of what everything at or below this node owes.
    pub fn subtree(&self) -> Dirty {
        unpack(self.0.load(Ordering::Acquire)).1
    }

    /// Both halves, read in one atomic load, as `(own, subtree)`.
    ///
    /// A walk that tests the subtree union and then reads the own bits wants this rather than two
    /// loads, which could straddle a concurrent mark and disagree with each other.
    pub fn get(&self) -> (Dirty, Dirty) {
        unpack(self.0.load(Ordering::Acquire))
    }

    /// Whether this node and everything below it owe nothing.
    pub fn is_clean(&self) -> bool {
        self.0.load(Ordering::Acquire) == 0
    }

    /// Adds `bits` to this node's own obligations and to its subtree union.
    ///
    /// Returns `true` when the subtree union gained a bit it did not already have, which is the
    /// signal that propagation to the parent is still necessary.
    pub fn mark(&self, bits: Dirty) -> bool {
        let previous = self.0.fetch_or(both(bits), Ordering::AcqRel);
        !unpack(previous).1.contains(bits)
    }

    /// Adds `bits` to the subtree union only. Returns `true` when the union changed.
    pub fn mark_subtree(&self, bits: Dirty) -> bool {
        let previous = self.0.fetch_or(subtree_half(bits), Ordering::AcqRel);
        !unpack(previous).1.contains(bits)
    }

    /// Removes `bits` from this node's own obligations, leaving the subtree union alone.
    pub fn clear_own(&self, bits: Dirty) {
        self.0.fetch_and(!(bits.bits() as u64), Ordering::AcqRel);
    }

    /// Clears `phase` from the subtree union, then re-adds `keep`, so a phase walk can retire one
    /// phase on unwind without clobbering the other bits' pending unions.
    ///
    /// `keep` is what the walk found still outstanding below this node — a descendant that
    /// re-marked itself while it was being serviced — and is added back in the same atomic step
    /// the retirement happens in, so a concurrent reader never observes the union without it.
    pub fn retire_phase(&self, phase: Dirty, keep: Dirty) {
        let cleared = !subtree_half(phase);
        let restored = subtree_half(keep);
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |word| {
                Some((word & cleared) | restored)
            });
    }
}

impl Default for DirtyCell {
    fn default() -> Self {
        Self::clean()
    }
}

impl fmt::Debug for DirtyCell {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (own, subtree) = self.get();
        formatter
            .debug_struct("DirtyCell")
            .field("own", &own)
            .field("subtree", &subtree)
            .finish()
    }
}

#[cfg(test)]
mod tests;
