//! What an implementation resourced a frame's vector work into.

use zgui_scene::{PlannedItem, ScenePassPlan};

use crate::vector::pass::VectorPass;

/// A frame's vector work, resourced.
///
/// It is the display list's plan with an implementation's own decisions attached — which scratch
/// each pass writes into, and which region of the surface that scratch stands for. The items are
/// carried through unchanged, because which clip each one applies inside the scratch is not the
/// implementation's decision either.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VectorPlan {
    /// The passes, in the order they will be executed.
    pub passes: Vec<VectorPass>,
    /// Every pass's items, in draw order.
    pub items: Vec<PlannedItem>,
}

impl VectorPlan {
    /// A plan with no work in it.
    ///
    /// The case worth caring about: a frame with no paths must cost no rasterisation work at all,
    /// because even a deliberately empty pass over a full-size surface costs real time and rather
    /// more latency.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether there is nothing to rasterise.
    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }

    /// How many passes the frame costs.
    pub fn len(&self) -> usize {
        self.passes.len()
    }

    /// The items of one pass.
    pub fn items_of(&self, pass: &VectorPass) -> &[PlannedItem] {
        &self.items[pass.items.clone()]
    }

    /// Starts a plan from the display list's, with the items carried through and no passes yet.
    ///
    /// An implementation fills in the passes as it resources them, one per planned pass and in the
    /// same order: a composite is named by its index, so a plan that skipped a pass would draw every
    /// later composite from the wrong one. Taking the items from here rather than rebuilding them is
    /// what keeps the two plans describing the same work.
    pub fn resourcing(scene: &ScenePassPlan) -> Self {
        Self {
            passes: Vec::with_capacity(scene.passes.len()),
            items: scene.items.clone(),
        }
    }
}
