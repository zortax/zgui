//! The outlines a drawing draws, from the text they are written in to the curves a rasteriser
//! flattens.
//!
//! A drawing arrives as path notation or as a document on an element. Its outlines stay in their
//! own space, and the fit that places them in the fragment's space travels beside them. Parsing per
//! frame would be a parse per icon per frame — and worse than the cost, it would hand the
//! rasteriser a new allocation every frame, which its encoding cache recognises by identity and
//! would therefore miss every single time. So a source text is read once for every element that
//! draws it, and the *same* shared paths are handed back for as long as the text stands. A new box
//! is a new fit and no new path.
//!
//! # Why this is a trait and not a function over the document
//!
//! The emit walk is a pure reader with no document in it — that is what lets a paint test drive the
//! real walk over a fixture with no cascade behind it. So the walk asks a source, and the document
//! is one implementation of it.

mod cache;
mod cpu;
mod layer;
mod mask;
pub(crate) mod path_glyphs;
pub(crate) mod payloads;
mod recognitions;

pub use crate::content::vectors::cache::{VectorCache, Vectors};
#[doc(hidden)]
pub use crate::content::vectors::layer::{
    LayerAnswer, LayerFallback, LayerRequest, Tiles, VectorLayerSource,
};
pub(crate) use crate::content::vectors::layer::{VectorLayerCache, phase_of};
#[doc(hidden)]
pub use crate::content::vectors::mask::CachedMarks;
pub(crate) use crate::content::vectors::mask::VectorMaskCache;
pub use crate::content::vectors::mask::{
    AnalyticOnly, MarksOnly, NoVectorMasks, VectorMask, VectorMaskRequest, VectorMaskSource,
    VectorMaskStyle,
};
#[doc(hidden)]
pub use crate::content::vectors::path_glyphs::{GlyphRequest, GlyphSheets, Splits};
#[doc(hidden)]
pub use crate::content::vectors::payloads::MarkPayloads;
pub(crate) use crate::content::vectors::recognitions::PartKey;
#[doc(hidden)]
pub use crate::content::vectors::recognitions::Recognitions;

use std::sync::{Arc, OnceLock};

use zgui_dom::NodeKey;
use zgui_scene::kurbo::{Affine, BezPath};

/// The outlines one element draws, and the matrix that places them in its fragment's own space.
#[derive(Clone, Debug, Default)]
pub struct Drawing {
    /// One outline per entry, in the order they are painted.
    ///
    /// Each carries its own paint, its own stroke style and its own clips, because a vector
    /// document is a picture and not a silhouette: a two-colour logo whose shapes shared one paint
    /// would be a one-colour logo. An element that draws plain path notation produces shapes whose
    /// paint is the inherited one, which is the same list with the same type in it.
    ///
    /// The outlines are shared rather than owned: every element drawing the same source draws the
    /// same allocation, and a rasteriser keeps its encoding of a path under the identity of the
    /// path's allocation.
    pub shapes: Arc<[zgui_svg::Shape]>,
    /// What places [`shapes`](Self::shapes) in the fragment's space: the identity for shapes
    /// already placed.
    ///
    /// For a canvas, the box fit times the scene's view transform.
    pub fit: Affine,
    /// The series of a canvas, each with the number of shapes painted under it.
    pub series: Arc<[zgui_canvas::SeriesAt]>,
    /// Each shape placed by `fit`, made on first use and shared by every clone.
    placed: Arc<[OnceLock<zgui_svg::Shape>]>,
}

impl Drawing {
    /// A drawing of shapes already in the fragment's space.
    pub fn placed_shapes(shapes: Vec<zgui_svg::Shape>) -> Self {
        Self {
            shapes: Arc::from(shapes),
            fit: Affine::IDENTITY,
            series: Arc::from([]),
            placed: Arc::from([]),
        }
    }

    /// A drawing of `shapes` in their own space, placed by `fit`.
    ///
    /// Every route draws from the source shape, so a hundred thousand markers never become a
    /// second path. Only recognition under a fit that scales its axes differently places a shape.
    pub fn fitted(shapes: Vec<zgui_svg::Shape>, fit: Affine) -> Self {
        Self::fitted_shared(Arc::from(shapes), fit)
    }

    /// The same as [`Drawing::fitted`], for shapes another drawing may share.
    pub fn fitted_shared(shapes: Arc<[zgui_svg::Shape]>, fit: Affine) -> Self {
        let placed = (0..shapes.len()).map(|_| OnceLock::new()).collect();
        Self {
            shapes,
            fit,
            series: Arc::from([]),
            placed,
        }
    }

    /// The drawing of a canvas scene, placed by `fit` times the scene's view transform.
    ///
    /// The shapes and the series keep their source allocations, so a pan or a zoom alone
    /// recognises nothing again, builds no new series payload and encodes no general shape again.
    /// A mask-route shape rasterises again under each new view, and recognition places a shape
    /// again under a turn or a stretch.
    pub fn canvas(scene: &zgui_canvas::CanvasScene, fit: Affine) -> Self {
        Self {
            series: Arc::from(scene.series()),
            ..Self::fitted(scene.shapes().to_vec(), fit * scene.transform())
        }
    }

    /// Shape `index` placed in the fragment's space.
    ///
    /// Placed on first use and kept for as long as the drawing is held. Recognition under a fit
    /// that scales its axes differently reads it, and so do tests.
    ///
    /// # Panics
    ///
    /// Panics when `index` is not a shape of the drawing.
    pub fn placed(&self, index: usize) -> &zgui_svg::Shape {
        let shape = &self.shapes[index];
        if self.fit == Affine::IDENTITY {
            return shape;
        }
        self.placed[index].get_or_init(|| zgui_svg::document::place::shape(shape, self.fit))
    }

    /// Every shape placed in the fragment's space, in painting order.
    pub fn placed_all(&self) -> Vec<zgui_svg::Shape> {
        (0..self.shapes.len())
            .map(|index| self.placed(index).clone())
            .collect()
    }

    /// The cell shape `index` is placed into, for a drawing that places its shapes.
    pub(crate) fn cell(&self, index: usize) -> Option<&OnceLock<zgui_svg::Shape>> {
        self.placed.get(index)
    }
}

/// Where the outlines an element draws come from.
pub trait VectorSource {
    /// The outlines `node` draws, or nothing if it draws none.
    ///
    /// Called once per drawing fragment the damage reaches. An implementation that parses on every
    /// call is correct and slow; the one this framework installs memoises by node.
    fn drawing(&self, node: NodeKey, placement: Placement) -> Option<Drawing>;

    /// A fingerprint of what `node` draws, moved whenever the drawing's source data changes.
    ///
    /// Read into the paint record's signature. A drawing fragment names its node and nothing
    /// about the curves, so this is what makes a swapped icon a cache miss for a record kept
    /// across frames. A source whose drawings never change may leave the default.
    fn revision(&self, _node: NodeKey) -> u64 {
        0
    }
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
