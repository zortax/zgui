//! What makes a handle usable as a table's key.

use crate::id::{ClipId, PaintId};

/// A `u32` handle a [`Table`](crate::Table) can hand out.
///
/// It exists so that one table implementation serves several id spaces without any of them being
/// interchangeable: the table is generic over this trait, and a `PaintId` is not a `ClipId` even
/// though both are one integer wide.
pub trait TableId: Copy + Eq {
    /// The handle for a slot.
    fn from_index(index: u32) -> Self;

    /// The slot a handle refers to.
    fn index(self) -> u32;
}

/// Implements [`TableId`] for a handle that is a transparent `u32`.
macro_rules! table_id {
    ($($name:ty),+ $(,)?) => {
        $(
            impl TableId for $name {
                fn from_index(index: u32) -> Self {
                    Self(index)
                }

                fn index(self) -> u32 {
                    self.0
                }
            }
        )+
    };
}

table_id!(ClipId, PaintId);
