//! References to paint table entries.

use zgui_space::PaintId;

/// The family of paint a [`PaintRef`] refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PaintKind {
    /// Nothing is painted.
    None = 0,
    /// One colour everywhere.
    Solid = 1,
    /// A ramp between colour stops.
    Gradient = 2,
    /// One colour per border side, see [`Paint::Sides`](crate::paints::Paint::Sides).
    Sides = 3,
}

/// A reference to a paint table entry: its family and its index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

    /// Returns a reference to `id` of the family `kind`.
    pub const fn new(kind: PaintKind, id: PaintId) -> Self {
        Self {
            kind: kind as u32,
            index: id.0,
        }
    }

    /// Returns a reference to the solid entry `id`.
    pub const fn solid(id: PaintId) -> Self {
        Self::new(PaintKind::Solid, id)
    }

    /// Reports whether this reference paints nothing.
    pub const fn is_none(self) -> bool {
        self.kind == PaintKind::None as u32
    }

    /// Returns the entry this reference refers to, or `None` when it paints nothing.
    pub const fn id(self) -> Option<PaintId> {
        if self.is_none() {
            None
        } else {
            Some(PaintId(self.index))
        }
    }
}
