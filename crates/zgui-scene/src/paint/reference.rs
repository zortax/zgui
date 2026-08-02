//! The eight bytes a primitive spends on paint.

use bytemuck::{Pod, Zeroable};

use crate::id::PaintId;

/// Which family a [`PaintRef`] points into.
///
/// It travels in the instance beside the index so that a shader can branch without first reading
/// the table, which is the whole saving: a solid fill never touches the paint storage at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PaintKind {
    /// Nothing is painted. The index is meaningless.
    None = 0,
    /// One colour everywhere.
    Solid = 1,
    /// A ramp between colour stops.
    Gradient = 2,
    /// A sampled image.
    Image = 3,
}

/// A primitive's reference to its paint: a family and an index.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct PaintRef {
    /// The [`PaintKind`] discriminant.
    pub kind: u32,
    /// The [`PaintId`] index, meaningless when `kind` is [`PaintKind::None`].
    pub index: u32,
}

impl PaintRef {
    /// A reference that paints nothing.
    pub const NONE: Self = Self {
        kind: PaintKind::None as u32,
        index: 0,
    };

    /// A reference to `id`, whose family the caller already knows.
    pub const fn new(kind: PaintKind, id: PaintId) -> Self {
        Self {
            kind: kind as u32,
            index: id.0,
        }
    }

    /// A reference to a solid-colour entry.
    ///
    /// [`PaintTable::reference`](crate::PaintTable::reference) is the general version, which reads
    /// the family off the entry instead of taking the caller's word for it.
    pub const fn solid(id: PaintId) -> Self {
        Self {
            kind: PaintKind::Solid as u32,
            index: id.0,
        }
    }

    /// Whether this reference paints nothing.
    pub const fn is_none(self) -> bool {
        self.kind == PaintKind::None as u32
    }

    /// The entry this reference points at, or `None` when it paints nothing.
    pub const fn id(self) -> Option<PaintId> {
        if self.is_none() {
            None
        } else {
            Some(PaintId(self.index))
        }
    }
}
