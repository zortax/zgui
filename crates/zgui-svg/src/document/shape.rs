//! One outline of a resolved document, and everything that decides how it is drawn.

use std::sync::Arc;

use crate::document::gradient::Gradient;
use crate::document::ink::Ink;

/// What paints an outline.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// One colour everywhere.
    Solid(Ink),
    /// A ramp between colour stops.
    Gradient(Gradient),
}

impl Paint {
    /// The same paint at `factor` of its alpha.
    pub fn faded(self, factor: f32) -> Self {
        match self {
            Self::Solid(ink) => Self::Solid(ink.faded(factor)),
            Self::Gradient(gradient) => Self::Gradient(gradient.faded(factor)),
        }
    }

    /// Whether any part of this takes its colour from the element that draws the document.
    pub fn is_inherited(&self) -> bool {
        match self {
            Self::Solid(ink) => ink.is_inherited(),
            Self::Gradient(gradient) => gradient.is_inherited(),
        }
    }
}

/// How an outline is filled.
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    /// What it is filled with.
    pub paint: Paint,
    /// How it decides what is inside itself.
    pub rule: peniko::Fill,
}

/// How an outline is stroked.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// What the stroke is painted with.
    pub paint: Paint,
    /// The width, caps, joins, miter limit and dashes, all of them.
    ///
    /// One type rather than a width beside four other fields, because the geometry a stroke stands
    /// for is decided by all of them together and a rasteriser that read only the width would draw
    /// a dashed round-capped line as a solid butt-capped one with nothing reporting it.
    pub style: kurbo::Stroke,
}

/// One region of the document a clip keeps content inside.
///
/// The list on a shape is an intersection, in the order the clips were met walking down: an
/// element inside two clipped groups is inside both of them.
#[derive(Clone, Debug)]
pub struct Clip {
    /// The outline content is kept inside, in the document's own coordinates.
    pub path: Arc<kurbo::BezPath>,
    /// How that outline decides what is inside it.
    pub rule: peniko::Fill,
}

/// One outline of a document, with its paint and its clips already resolved.
///
/// A shape is flat: every group transform above it has been applied to its geometry, every group
/// opacity has been folded into its paint, and every clip it is inside is in its own list. That is
/// what lets a consumer draw a document by walking a list rather than by implementing a tree — and
/// what keeps the document model from needing a group concept a rasteriser would have to match.
#[derive(Clone, Debug)]
pub struct Shape {
    /// The outline, in the document's own coordinates.
    pub path: Arc<kurbo::BezPath>,
    /// What fills it, if anything.
    pub fill: Option<Fill>,
    /// What strokes it, if anything.
    pub stroke: Option<Stroke>,
    /// Every clip it is inside, which apply together.
    pub clips: Vec<Clip>,
}

impl Shape {
    /// Whether any of this shape's paint takes its colour from the element that draws it.
    pub fn is_inherited(&self) -> bool {
        let inherited = |paint: Option<&Paint>| paint.is_some_and(Paint::is_inherited);
        inherited(self.fill.as_ref().map(|fill| &fill.paint))
            || inherited(self.stroke.as_ref().map(|stroke| &stroke.paint))
    }
}
