//! The identifiers primitives, fragments and side tables address each other by.
//!
//! The handles that fragments and side tables share are defined in [`zgui_space::id`] and
//! re-exported here. The two below belong to the display list.

pub use zgui_space::id::{ClipId, PaintId, PaintSlot, ScrollFrameId, StackingContextId};

/// A primitive's position in the painting order.
///
/// Higher draws later, so higher wins where two primitives overlap. Two primitives at *equal*
/// order are provably non-overlapping — [`BoundsTree`](crate::BoundsTree) is what makes that true —
/// and at equal order the primitive kind decides the sequence, purely so that a batch of one kind
/// stays contiguous.
pub type DrawOrder = u32;

/// One piece of vector content, stable across frames.
///
/// A rasteriser keeps its own encoded form of the geometry under this id and re-places it each
/// frame instead of re-encoding it, so this crate never has to know what that encoded form is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VectorId(pub u32);

impl VectorId {
    /// The handle's numeric value, for indexing and for transcripts.
    pub const fn index(self) -> u32 {
        self.0
    }
}
