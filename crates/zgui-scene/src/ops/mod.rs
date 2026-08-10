//! The paint-operation log: what was inserted, in the order it was inserted.

use crate::prim::PrimitiveKind;

/// One entry of the log: which array a primitive went into, and where.
///
/// The log is the emission order itself, which is what the vector-pass policy sweeps: knowing
/// that a quad was emitted between two paths is not recoverable from the sorted arrays
/// afterwards. A chunk uses the same entries with its own arrays behind them — see
/// [`ChunkPrims`](crate::ChunkPrims) — which is how a fragment's painting outlives the frame's
/// log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintOp {
    /// Which array the primitive went into.
    pub kind: PrimitiveKind,
    /// Its index in that array at the time it was inserted.
    pub index: u32,
}

impl PaintOp {
    /// An entry for the primitive at `index` of `kind`'s array.
    pub const fn new(kind: PrimitiveKind, index: u32) -> Self {
        Self { kind, index }
    }
}
