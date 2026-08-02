//! A parsed document: what it draws, how big its own space is, and what it asked for that this
//! model cannot say.

pub mod gradient;
pub mod ink;
pub mod place;
pub mod shape;

use crate::document::shape::Shape;

/// What a document asked for that a flat list of painted outlines cannot express.
///
/// Counted rather than dropped in silence. A document whose whole picture is a filter or a
/// bitmap would otherwise parse cleanly, draw nothing, and give a caller no way to tell that from
/// an empty file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Unsupported {
    /// `<text>` and `<tspan>` elements, which this crate does not shape.
    pub text: u32,
    /// `<image>` elements, which are raster content rather than outlines.
    pub images: u32,
    /// Groups carrying a `mask`, drawn without it.
    pub masks: u32,
    /// Groups carrying a `filter`, drawn without it.
    pub filters: u32,
    /// Groups carrying a `mix-blend-mode` other than normal, drawn without it.
    pub blend_modes: u32,
    /// Outlines painted with a `<pattern>`, left unpainted.
    pub patterns: u32,
}

impl Unsupported {
    /// Whether the document asked for nothing outside the model.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// One parsed SVG document, as outlines a path rasteriser can draw.
///
/// The document keeps its own coordinate space rather than being resolved into any particular box:
/// [`Document::view_box`] is the rectangle the outlines are written inside, and fitting that
/// rectangle onto an element's content box is the caller's, once, at whatever size the element
/// turned out to be. A document parsed once is therefore drawn at every size, and re-colouring it
/// re-parses nothing at all.
#[derive(Clone, Debug)]
pub struct Document {
    /// The extent of the space the outlines are written in.
    size: (f64, f64),
    /// The outlines, in the order they are painted.
    shapes: Vec<Shape>,
    /// What the document asked for that this model does not carry.
    unsupported: Unsupported,
}

impl Document {
    /// A document of the given intrinsic extent, drawing `shapes`.
    pub fn new(size: (f64, f64), shapes: Vec<Shape>, unsupported: Unsupported) -> Self {
        Self {
            size,
            shapes,
            unsupported,
        }
    }

    /// The rectangle the outlines are written inside, as minimum x, minimum y, width and height.
    ///
    /// Always rooted at the origin: an SVG `viewBox` with an offset origin, and the
    /// `preserveAspectRatio` deciding how that box maps onto the document's own width and height,
    /// have both already been applied to the geometry. What is left is the box a caller fits to an
    /// element, which is the document's intrinsic size.
    pub fn view_box(&self) -> [f32; 4] {
        [0.0, 0.0, self.size.0 as f32, self.size.1 as f32]
    }

    /// The outlines, in the order they are painted.
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// Whether the document draws nothing at all.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// What the document asked for that this model does not carry.
    pub fn unsupported(&self) -> Unsupported {
        self.unsupported
    }

    /// Every outline of the document, moved by `matrix` into the space it is drawn in.
    ///
    /// Placing is a whole-shape operation and not a matrix handed to a rasteriser beside the
    /// geometry: the outlines move, the clips move with them, the ramps move with the shapes they
    /// paint, and the stroke widths and dash lengths scale. A consumer that applied the matrix to
    /// the outlines alone would draw a scaled icon with a hairline stroke and a ramp still sitting
    /// where the document wrote it.
    pub fn placed(&self, matrix: kurbo::Affine) -> Vec<Shape> {
        self.shapes
            .iter()
            .map(|shape| place::shape(shape, matrix))
            .collect()
    }

    /// Whether any of the document's paint takes its colour from the element that draws it.
    ///
    /// The question a caller asks to know whether a colour change has to reach this document at
    /// all: a logo with its own colours is unaffected by the text colour around it, and a
    /// `currentColor` icon is nothing but affected by it.
    pub fn is_inherited(&self) -> bool {
        self.shapes.iter().any(Shape::is_inherited)
    }
}
