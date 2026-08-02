//! One rasterisation pass, as an implementation has resourced it.

use core::ops::Range;

use zgui_geom::{Device, Rect};
use zgui_scene::ClipId;

use crate::vector::target::VectorTarget;

/// One pass of vector work, with an implementation's resources attached.
///
/// Everything about *which* items are in it, where it begins and ends, what it clips through and
/// whether it may be composited one item at a time was decided in the display list. What an
/// implementation adds here is only its own resourcing: which scratch the result went into, and
/// which region of the surface that scratch corresponds to.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorPass {
    /// The region of the surface this pass covers, aligned outwards so a tile grid lines up.
    pub region: Rect<i32, Device>,
    /// Where the implementation put the result.
    pub target: VectorTarget,
    /// The pass's items, indexing the plan's item list.
    pub items: Range<usize>,
    /// The clip a whole-pass composite binds.
    ///
    /// One draw applies one clip, so it has to be one every item of the pass genuinely has;
    /// everything below it was applied inside the scratch as each item's residual. Copied from the
    /// display list's plan, never decided here. Unused when the pass is composited per item, since
    /// each instance then binds its own.
    pub clip: ClipId,
    /// Whether to composite this pass with one draw per item, each reading only that item's part of
    /// the scratch and binding that item's own clip.
    ///
    /// Copied from the display list's plan, never decided here. It is sound only because the plan
    /// sets it exactly when no two of the pass's items overlap each other, so no part of the
    /// scratch is composited twice.
    pub instanced: bool,
}
