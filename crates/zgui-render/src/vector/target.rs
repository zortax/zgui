//! Where a rasteriser put a pass's coverage.

/// An opaque reference to wherever one implementation put one pass's result.
///
/// Deliberately not an array-layer index, and deliberately not a texture handle. One rasteriser
/// writes into layers of an array texture, another resolves multisampled scratch into a plain one;
/// naming either shape here would make the other one wrong. A compositing draw resolves this back
/// to something bindable through the implementation that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VectorTarget(pub u64);

impl VectorTarget {
    /// A pass an implementation had nowhere at all to put.
    ///
    /// A pass carrying this was **not** rasterised, and compositing it would draw whatever the
    /// scratch happens to hold — the previous frame's paths, in the right place, with nothing to
    /// notice them by. An implementation that hands one out says so by failing the frame's
    /// preparation with [`VectorError::OutOfCapacity`](crate::VectorError::OutOfCapacity), which is
    /// what shortens the plan to the passes that exist and counts the rest as undrawn.
    pub const NONE: Self = Self(u64::MAX);
}
