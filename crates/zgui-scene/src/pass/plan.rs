//! The plan a rasteriser executes.

use core::ops::Range;

use zgui_geom::{Device, Rect};

use crate::id::{ClipId, DrawOrder};

/// One item of a planned pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedItem {
    /// Which of the scene's vector items this is.
    pub item: usize,
    /// The part of the item's clip chain **below** the pass's, which has to be applied inside
    /// whatever draws the item.
    ///
    /// [`ClipId::ROOT`] — that is, nothing — for every item whose whole chain is the pass's, which
    /// is the common case and costs nothing. Anything else becomes a clip layer inside the vector
    /// content, which is what keeps a row of twelve differently clipped avatars at one pass instead
    /// of twelve.
    pub residual: ClipId,
    /// The clip this item's own composite instance binds, when its pass is composited per item.
    ///
    /// Ignored for a pass composited as one draw, which binds the pass's clip instead.
    pub clip: ClipId,
    /// What this item paints, relative to its pass's region: the source rectangle its composite
    /// instance reads.
    pub ink: Rect<i32, Device>,
}

/// One planned rasterisation pass.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedPass {
    /// This pass's items, indexing [`ScenePassPlan::items`].
    pub items: Range<usize>,
    /// The region of the surface this pass covers, aligned outwards so a tile grid lines up.
    pub region: Rect<i32, Device>,
    /// The deepest chain every item of this pass applies.
    ///
    /// A pass composited as one draw binds exactly one clip, so it has to be one every item
    /// genuinely has; everything below it is each item's [`PlannedItem::residual`]. A common chain
    /// always exists, because at worst it is [`ClipId::ROOT`].
    pub clip: ClipId,
    /// Whether this pass may be composited as one draw per item rather than one for the pass.
    ///
    /// True exactly when no two of its items overlap each other. Under that condition each item's
    /// quad reads a disjoint part of the scratch, so compositing them separately is pixel-identical
    /// to compositing the pass once — and it lets each item carry its own clip. When two items do
    /// overlap, per-item quads would blend the shared scratch twice over the overlap, so the flag
    /// is false and the whole pass is composited by one draw.
    pub instanced: bool,
    /// Where in the painting order the composite belongs: the draw order of the pass's last item.
    pub composite_order: DrawOrder,
}

/// Everything a frame's vector work amounts to.
///
/// Produced when the scene is finished, from the display list and the damage set, and handed to a
/// rasteriser to execute rather than to re-derive.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScenePassPlan {
    /// Every surviving item, flattened in draw order and grouped by pass.
    pub items: Vec<PlannedItem>,
    /// The passes, in draw order.
    pub passes: Vec<PlannedPass>,
    /// How many items the damage cull dropped.
    pub culled: usize,
    /// How many clip layers absorbing residuals will cost.
    ///
    /// Recorded beside the pass count because absorbing a clip *moves* the cost of a distinctly
    /// clipped item out of the pass count rather than deleting it, and a budget watching only
    /// passes would read that trade as a free win.
    pub clip_layers: usize,
}

impl ScenePassPlan {
    /// Whether there is no vector work at all this frame.
    ///
    /// A frame with no paths must cost no rasterisation work: even a deliberately empty pass over a
    /// full-size surface costs tens of microseconds of processor time and a great deal more
    /// latency, so this is worth asking before anything else.
    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }

    /// How many passes the frame costs.
    pub fn len(&self) -> usize {
        self.passes.len()
    }

    /// The items of one pass.
    pub fn items_of(&self, pass: &PlannedPass) -> &[PlannedItem] {
        &self.items[pass.items.clone()]
    }

    /// Empties the plan, keeping its allocations for the next frame.
    pub fn clear(&mut self) {
        self.items.clear();
        self.passes.clear();
        self.culled = 0;
        self.clip_layers = 0;
    }
}
