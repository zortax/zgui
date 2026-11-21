//! The identity of vector content.

/// One piece of vector content, stable across frames.
///
/// The caches key what they derived from a drawing, such as its recognised shapes, rasterised
/// layers and mark payloads, by this id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VectorId(pub u32);

impl VectorId {
    /// Returns the handle's numeric value.
    pub const fn index(self) -> u32 {
        self.0
    }
}
