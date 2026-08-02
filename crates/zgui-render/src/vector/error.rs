//! Why a rasteriser could not do as it was asked.

/// A vector rasterisation failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VectorError {
    /// The work exceeded a fixed capacity of the implementation.
    ///
    /// Named separately because it is the failure that is easiest to report as success: an
    /// implementation whose buffers overflow may return without writing anything, and a scratch that
    /// was not cleared first would then read as the *previous* frame's content — wrong pixels rather
    /// than missing ones, with nothing to notice it by.
    #[error("the rasteriser ran out of capacity: {detail}")]
    OutOfCapacity {
        /// What ran out.
        detail: String,
        /// How many of the plan's passes, counted from the first, were finished before it did.
        ///
        /// Those passes are in their scratches and are safe to composite; the rest are not, and a
        /// caller that keeps this many and drops the remainder draws what fits instead of losing
        /// the frame's vector content altogether.
        prepared: usize,
    },
    /// A resource could not be created.
    #[error("the rasteriser could not allocate: {detail}")]
    Allocation {
        /// What could not be allocated.
        detail: String,
    },
    /// The device rejected something, or was lost.
    #[error("the device rejected the rasterisation: {detail}")]
    Device {
        /// What the device said.
        detail: String,
    },
}
