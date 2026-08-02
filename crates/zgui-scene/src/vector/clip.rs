//! An outline a vector item is kept inside.

use std::sync::Arc;

/// One arbitrary-shape clip on a single vector item.
///
/// # Why this is not a clip chain link
///
/// A [`ClipId`](crate::ClipId) chain is what every primitive in the document is clipped by, and it
/// is deliberately made of shapes a fragment shader can evaluate: rectangles, elliptical corners
/// and a sampled coverage tile. An arbitrary Bézier outline is none of those, and putting one in
/// that chain would mean either a chain a shader cannot answer or a rectangle standing in for a
/// shape, which is a wrong pixel with nothing reporting it.
///
/// So a shape clip lives on the item instead — where it is a clip on *vector content only*, applied
/// by the path rasteriser that is already flattening outlines, which is the one consumer for which
/// an arbitrary outline costs nothing new. Both rasterisers apply it; neither needs a new concept
/// to do so.
///
/// The outline is in the same space as the item's own path and is applied under the item's own
/// transform, so a clipped drawing that is rotated has its clip rotated with it.
#[derive(Clone, Debug)]
pub struct VectorClip {
    /// The outline content is kept inside.
    pub path: Arc<kurbo::BezPath>,
    /// How that outline decides what is inside it.
    pub rule: peniko::Fill,
}

impl VectorClip {
    /// A clip keeping content inside `path` by the non-zero rule.
    pub fn new(path: Arc<kurbo::BezPath>) -> Self {
        Self {
            path,
            rule: peniko::Fill::NonZero,
        }
    }

    /// The same clip evaluated by the even-odd rule.
    pub fn even_odd(mut self) -> Self {
        self.rule = peniko::Fill::EvenOdd;
        self
    }
}
