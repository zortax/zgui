//! What can go wrong while caching a raster.

use zgui_geom::{Device, Size};

use crate::sink::SinkError;

/// Why an atlas operation could not be completed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AtlasError {
    /// The content is larger than any texture this atlas is allowed to create.
    ///
    /// This is not recoverable by evicting: no arrangement of the pool has room for it. The caller
    /// has to rasterise smaller, or draw the content some other way.
    #[error(
        "a {}x{} tile does not fit a texture capped at {}x{}",
        requested.width,
        requested.height,
        limit.width,
        limit.height
    )]
    TooLarge {
        /// The size that was asked for.
        requested: Size<i32, Device>,
        /// The largest texture this atlas may create.
        limit: Size<i32, Device>,
    },
    /// Every texture of the pool is full and no more may be created.
    ///
    /// Evicting and retrying is the response: unlike [`AtlasError::TooLarge`], the content does fit
    /// somewhere, just not in what is currently allocated.
    #[error("the pool has no room for a {}x{} tile", requested.width, requested.height)]
    OutOfSpace {
        /// The size that was asked for.
        requested: Size<i32, Device>,
    },
    /// A tile's size and the byte count supplied for it disagree.
    #[error("a {}x{} tile of this format needs {expected} bytes, not {actual}", size.width, size.height)]
    WrongByteCount {
        /// The tile's size.
        size: Size<i32, Device>,
        /// How many bytes the format requires for that size.
        expected: u64,
        /// How many bytes were supplied.
        actual: u64,
    },
    /// The sink refused an operation.
    #[error(transparent)]
    Sink(#[from] SinkError),
}
