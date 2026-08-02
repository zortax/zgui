//! The outlines a drawing draws, from the text they are written in to the curves a rasteriser
//! flattens.
//!
//! A drawing arrives as path notation on an element, and what a rasteriser wants is Béziers already
//! placed in the fragment's own space. Doing that conversion per frame would be a parse and a
//! re-place per icon per frame — and worse than the cost, it would hand the rasteriser a new
//! allocation every frame, which its encoding cache recognises by identity and would therefore miss
//! on every single time. So the conversion is memoised here, by node, and the *same* shared path is
//! handed back for as long as neither the notation nor the box it is fitted to has moved.
//!
//! # Why this is a trait and not a function over the document
//!
//! The emit walk is a pure reader with no document in it — that is what lets a paint test drive the
//! real walk over a fixture with no cascade behind it. So the walk asks a source, and the document
//! is one implementation of it.

mod cache;

pub use crate::content::vectors::cache::{VectorCache, Vectors};

use std::sync::Arc;

use zgui_dom::NodeKey;
use zgui_scene::kurbo::{Affine, BezPath};

/// The outlines one element draws, placed in its fragment's own space.
#[derive(Clone, Debug, Default)]
pub struct Drawing {
    /// One outline per entry, in the order they are painted.
    ///
    /// Each carries its own paint, its own stroke style and its own clips, because a vector
    /// document is a picture and not a silhouette: a two-colour logo whose shapes shared one paint
    /// would be a one-colour logo. An element that draws plain path notation produces shapes whose
    /// paint is the inherited one, which is the same list with the same type in it.
    ///
    /// The outlines are shared rather than owned: the same curves are drawn every frame, and a
    /// rasteriser keeps its encoding of them under the identity of the allocation.
    pub shapes: Vec<zgui_svg::Shape>,
}

/// Where the outlines an element draws come from.
pub trait VectorSource {
    /// The outlines `node` draws, or nothing if it draws none.
    ///
    /// Called once per drawing fragment the damage reaches. An implementation that parses on every
    /// call is correct and slow; the one this framework installs memoises by node.
    fn drawing(&self, node: NodeKey, placement: Placement) -> Option<Drawing>;
}

/// The box a drawing is being fitted to, and at what ratio.
///
/// Passed to the source rather than applied afterwards because fitting is part of what is cached:
/// the placed curves are what a rasteriser encodes, so a drawing re-placed into the same box must
/// hand back the allocation it handed back last frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The content box the outlines are fitted to, in the fragment's local space.
    pub content_box: zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>,
    /// How many device pixels one CSS pixel is.
    pub scale: f32,
}

/// A source with no drawings in it.
///
/// What a paint test that is not testing drawings uses, and what the walk defaults to — a walk that
/// silently invented outlines for a fixture would be a walk no fixture could hold still.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoVectors;

impl VectorSource for NoVectors {
    fn drawing(&self, _node: NodeKey, _placement: Placement) -> Option<Drawing> {
        None
    }
}

/// Reads a list of outlines the way an element carries them: one per line, each in path notation.
///
/// A line that does not parse is dropped rather than failing the whole list, because a drawing is
/// data a view computed and one bad mark must not take the other nine with it.
pub fn parse(data: &str) -> Vec<BezPath> {
    data.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| BezPath::from_svg(line).ok())
        .collect()
}

/// The shapes a list of plain outlines draws, placed by `placed`.
///
/// Every one of them takes the inherited paint, which is what makes `.icon:hover { color: … }`
/// re-colour a drawing with nothing else added — and what makes a drawing written as path notation
/// the degenerate vector document, rather than a second kind of content with rules of its own.
pub fn outlines(paths: &[BezPath], placed: Affine) -> Vec<zgui_svg::Shape> {
    paths
        .iter()
        .map(|path| zgui_svg::Shape {
            path: Arc::new(placed * path.clone()),
            fill: Some(zgui_svg::Fill {
                paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Inherited { alpha: 1.0 }),
                rule: zgui_scene::peniko::Fill::NonZero,
            }),
            stroke: None,
            clips: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use zgui_scene::kurbo::Shape;

    use super::parse;

    #[test]
    fn each_line_is_its_own_outline() {
        let paths = parse("M0 0 L8 0 L8 8 Z\nM2 2 L4 2 L4 4 Z");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].bounding_box().width(), 8.0);
        assert_eq!(paths[1].bounding_box().width(), 2.0);
    }

    /// One bad mark in a chart must not delete the other nine.
    #[test]
    fn a_line_that_does_not_parse_is_dropped_and_the_rest_survive() {
        let paths = parse("M0 0 L8 0\nnot a path at all\nM1 1 L2 2");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn nothing_at_all_is_no_outlines_rather_than_one_empty_one() {
        assert!(parse("").is_empty());
        assert!(parse("\n   \n").is_empty());
    }
}
