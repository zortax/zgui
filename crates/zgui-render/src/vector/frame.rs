//! Everything a rasteriser is handed for one frame.

use zgui_scene::{ClipTable, PaintTable, Placements, VectorItem};

use crate::vector::plan::VectorPlan;

/// One frame's vector work, and everything needed to execute it.
///
/// The side tables travel with the items because an item names its content by index into them and
/// nothing else can resolve those names. The clip table in particular is what a residual clip — the
/// part of an item's chain its pass's own clip does not cover — resolves through, and a second copy
/// of it would be a second answer to the same question.
pub struct VectorFrame<'frame> {
    /// The work, already resourced.
    pub plan: &'frame VectorPlan,
    /// The scene's vector items, which the plan indexes.
    pub items: &'frame [VectorItem],
    /// The chains the items' clips and residuals resolve through.
    pub clips: &'frame ClipTable,
    /// What fills and strokes the items.
    pub paints: &'frame PaintTable,
    /// The matrix each coordinate system the items draw in resolves to.
    pub placements: &'frame Placements,
}

impl<'frame> VectorFrame<'frame> {
    /// A frame of work over `items`, painted from `paints`, clipped through `clips`.
    pub fn new(
        plan: &'frame VectorPlan,
        items: &'frame [VectorItem],
        clips: &'frame ClipTable,
        paints: &'frame PaintTable,
        placements: &'frame Placements,
    ) -> Self {
        Self {
            plan,
            items,
            clips,
            paints,
            placements,
        }
    }

    /// Whether there is nothing to do.
    pub fn is_empty(&self) -> bool {
        self.plan.is_empty()
    }
}
