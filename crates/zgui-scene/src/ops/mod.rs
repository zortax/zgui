//! The paint-operation log: what was inserted, in the order it was inserted.

use crate::prim::PrimitiveKind;

/// One entry of the log: which array a primitive went into, and where.
///
/// The log exists so that a fragment whose appearance did not change can be *replayed* rather than
/// re-emitted. A fragment records the range of the log its primitives occupy; next frame, if
/// nothing about it changed, that range is copied forward — and if it merely moved, copied forward
/// with a translation. Re-encoding a scrolled list's five hundred rows is exactly the cost that
/// buys.
///
/// It is also the emission order itself, which is what the vector-pass policy sweeps: knowing that
/// a quad was emitted between two paths is not recoverable from the sorted arrays afterwards.
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
