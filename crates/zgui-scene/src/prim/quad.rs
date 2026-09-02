//! The rounded, bordered rectangle: the workhorse of CSS boxes.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Corners, Device, DevicePx, Rect, Size, Vec2};

use crate::id::{ClipId, DrawOrder};
use crate::paint::{PaintKind, PaintRef};
use crate::spatial::SpatialId;

/// How a border is drawn along its edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum BorderStyle {
    /// One continuous line.
    Solid = 0,
    /// Dashes with gaps between them, continuing around the corners.
    Dashed = 1,
    /// Round dots with gaps between them.
    Dotted = 2,
}

/// A rounded, bordered rectangle.
///
/// Its corner radii are elliptical — a horizontal and a vertical radius per corner — because that
/// is what `border-radius` specifies, and its fill and stroke are indices rather than colours, so a
/// gradient costs no more instance bytes than a flat colour.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Quad {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// The [`BorderStyle`] discriminant in the low byte, and the dash-phase index above it.
    pub style: u32,
    /// The rectangle, as `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// Elliptical corner radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
    /// Border widths, as `[top, right, bottom, left]`.
    pub border: [f32; 4],
    /// What fills the rectangle.
    pub fill: PaintRef,
    /// What draws its border.
    pub stroke: PaintRef,
    /// The [`ClipId`] this draws through.
    pub clip: u32,
    /// The slot of the [`SpatialId`] this draws under.
    pub transform: u32,
    /// The superellipse exponent its corners are cut with; two is the ellipse.
    ///
    /// Beside the radii rather than folded into them because it is the *shape* of the corner and
    /// they are its size: a squircle of twenty pixels and a circle of twenty pixels differ in this
    /// and in nothing else.
    pub shape: f32,
    /// Where the space its paints are described in has its origin, as `[x, y]`.
    ///
    /// A [`Paint`](crate::Paint) states its geometry — a gradient line, a ramp's centre, an image's
    /// destination — in the coordinates of the surface it was first resolved against, so a quad
    /// that has since been moved has to be sampled at the point it *was* at rather than the point
    /// it is at. This is that displacement, subtracted from the sample point before either paint is
    /// evaluated, so a ramp travels with the rectangle it fills instead of staying where the
    /// rectangle used to be. Zero for a quad drawn where its paints were resolved.
    pub paint_origin: [f32; 2],
}

impl Quad {
    /// A borderless, square-cornered rectangle filled with `fill`.
    pub fn filled(bounds: Rect<DevicePx, Device>, fill: PaintRef) -> Self {
        Self {
            order: 0,
            style: BorderStyle::Solid as u32,
            bounds: [
                bounds.origin.x.0,
                bounds.origin.y.0,
                bounds.size.width.0,
                bounds.size.height.0,
            ],
            radii: [0.0; 8],
            border: [0.0; 4],
            fill,
            stroke: PaintRef::NONE,
            shape: crate::prim::CornerShape::ROUND.get(),
            clip: ClipId::ROOT.0,
            transform: SpatialId::VIEWPORT.index(),
            paint_origin: [0.0, 0.0],
        }
    }

    /// The same quad drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip.0;
        self
    }

    /// Moves the space this quad's paints are described in by `by`, without touching either paint.
    ///
    /// The counterpart of translating [`bounds`](Self::bounds): a rectangle that moves takes its
    /// ramp with it, and this is how, at no cost in the paint table and none per paint. Repeated
    /// displacements accumulate, so a rectangle carried forward one scroll step at a time ends up
    /// anchored where it was first painted rather than where it was last painted.
    pub fn reanchor_paint(&mut self, by: Size<DevicePx, Device>) {
        self.paint_origin[0] += by.width.0;
        self.paint_origin[1] += by.height.0;
    }

    /// Whether either paint is read at the point being drawn rather than being one colour.
    ///
    /// A flat fill is the same colour everywhere, so where it is sampled cannot matter; a ramp and
    /// a sampled image are both read at a point, so both stop agreeing with their rectangle the
    /// moment it moves without them.
    pub fn samples_its_paint(&self) -> bool {
        [self.fill, self.stroke].into_iter().any(|paint| {
            paint.kind == PaintKind::Gradient as u32 || paint.kind == PaintKind::Image as u32
        })
    }

    /// The same quad drawn under `transform`.
    pub fn transformed(mut self, transform: SpatialId) -> Self {
        self.transform = transform.index();
        self
    }

    /// The same quad with its corners cut to `shape`.
    pub fn with_corner_shape(mut self, shape: crate::prim::CornerShape) -> Self {
        self.shape = shape.get();
        self
    }

    /// The shape its corners are cut with.
    pub fn corner_shape(&self) -> crate::prim::CornerShape {
        crate::prim::CornerShape(self.shape)
    }

    /// The same quad with elliptical corner radii.
    pub fn with_radii(mut self, radii: Corners<Vec2<DevicePx>>) -> Self {
        self.radii = [
            radii.top_left.x.0,
            radii.top_left.y.0,
            radii.top_right.x.0,
            radii.top_right.y.0,
            radii.bottom_right.x.0,
            radii.bottom_right.y.0,
            radii.bottom_left.x.0,
            radii.bottom_left.y.0,
        ];
        self
    }

    /// The same quad with a border of the given widths, paint and style.
    pub fn with_border(mut self, widths: [f32; 4], stroke: PaintRef, style: BorderStyle) -> Self {
        self.border = widths;
        self.stroke = stroke;
        self.style = (self.style & !0xff) | style as u32;
        self
    }

    /// The rectangle this paints, which is what draw order and culling are computed from.
    pub fn ink(&self) -> Rect<DevicePx, Device> {
        crate::prim::layout::rect_of(self.bounds)
    }

    /// The clip chain this draws through.
    pub fn clip_id(&self) -> ClipId {
        ClipId(self.clip)
    }
}
