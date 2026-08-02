//! Which target a planned pass draws into.

use crate::target::group_pool::GroupSlot;
use crate::target::scale::TargetScale;

/// Where a pass writes.
///
/// A frame writes into the composed target and into targets lent by the pool, and every one of
/// them holds the same device-pixel coordinates: a quad drawn inside an isolated group lands at
/// exactly the position it would have landed at without the isolation. What differs is only how
/// many texels a device pixel covers, which is why the resolution travels with the reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetRef {
    /// The persistent target the frame is composed into.
    Composed,
    /// A target lent by the pool for one group, one backdrop capture, or one blur pass.
    Pool(GroupSlot),
}

impl TargetRef {
    /// How many texels of this target one device pixel covers.
    pub fn scale(self) -> TargetScale {
        match self {
            Self::Composed => TargetScale::Full,
            Self::Pool(slot) => slot.scale(),
        }
    }

    /// The pool target this names, if it is one.
    pub fn slot(self) -> Option<GroupSlot> {
        match self {
            Self::Composed => None,
            Self::Pool(slot) => Some(slot),
        }
    }
}
