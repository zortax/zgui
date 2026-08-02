//! Where each indexed entry is filed, across every tree in the forest.
//!
//! One table for the whole forest and not one per tree. Two reasons, and the first is the one that
//! decides it: a table addressed by fragment slot is as long as the highest slot it holds, so a
//! document with a thousand coordinate systems would pay a thousand tables each stretching to
//! wherever its own entries happen to fall — quadratic in the number of spaces, for a fact that
//! is one fact per entry. The second is that the fact really is forest-wide: an entry lives in
//! exactly one tree, and which one is exactly what a caller that has only a fragment name needs
//! before it can remove it.

use zgui_arena::SlotVec;
use zgui_scene::SpatialId;

use crate::fragment::FragKey;

/// Which tree holds one entry, and which of its leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Home {
    /// The coordinate system whose tree holds it.
    pub(crate) space: Option<SpatialId>,
    /// The leaf of that tree it sits in.
    pub(crate) leaf: usize,
}

/// Every entry's home, by fragment.
pub(crate) type Homes = SlotVec<FragKey, Home>;
