//! Many recognised shapes of one path, drawn from one shared payload.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Device, DevicePx, Rect, Size};

use crate::id::{ClipId, DrawOrder};
use crate::paint::{PaintKind, PaintRef};
use crate::spatial::SpatialId;

/// The bits of [`MarkItem::flags`].
#[derive(Clone, Copy, Debug)]
pub struct MarkFlags;

impl MarkFlags {
    /// The prims overlap, so the item paints the union of their coverage once.
    ///
    /// Without it, every prim is drawn on its own: the prims are apart, and no pixel takes
    /// coverage from two of them.
    pub const UNION: u32 = 1;
    /// Payload positions map by [`MarkItem::axes`] and [`MarkItem::origin`] to local space, and
    /// payload lengths are local units.
    ///
    /// Without it, the whole payload maps by `axes` and `origin`, and lengths scale with it.
    pub const SCREEN: u32 = 2;
    /// The discs are squares of half side `outer`, with a square hole of half side `inner`.
    pub const SQUARE_DISCS: u32 = 4;
    /// Where the cap of every run start is stored: two bits from bit 8.
    pub const START_CAP_SHIFT: u32 = 8;
    /// Where the cap of every run end is stored: two bits from bit 10.
    pub const END_CAP_SHIFT: u32 = 10;
    /// A cap that stops at the end of the segment.
    pub const BUTT: u32 = 0;
    /// A cap that continues the segment by half the stroke width.
    pub const SQUARE: u32 = 1;
    /// A half disc of half the stroke width.
    pub const ROUND: u32 = 2;

    /// The flag bits for runs that start with `start` and end with `end`.
    pub const fn caps(start: u32, end: u32) -> u32 {
        ((start & 3) << Self::START_CAP_SHIFT) | ((end & 3) << Self::END_CAP_SHIFT)
    }
}

/// One shape whose prims are discs, boxes and stroked polylines, drawn with one paint.
///
/// The geometry is in a [`MarkPayload`] beside the item, and the item holds only how many of
/// each kind the payload has. So a hundred thousand circles are one item: one draw order, one
/// clip, one paint and one ink rectangle.
///
/// Every payload coordinate maps to local space by [`axes`](Self::axes) and then
/// [`origin`](Self::origin). A replay that moves the item adds the offset to the origin and leaves
/// the payload as it is, so a payload is shared by every copy of the item.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct MarkItem {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// [`MarkFlags`] bits.
    pub flags: u32,
    /// What the prims paint together, as `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// What fills every prim.
    pub paint: PaintRef,
    /// The [`ClipId`] this draws through.
    pub clip: u32,
    /// The slot of the [`SpatialId`] this draws under.
    pub transform: u32,
    /// Where the space its paint is described in has its origin, as `[x, y]`.
    pub paint_origin: [f32; 2],
    /// What is added to every payload coordinate, as `[x, y]`.
    pub origin: [f32; 2],
    /// How many discs the payload holds.
    pub discs: u32,
    /// How many boxes the payload holds.
    pub boxes: u32,
    /// How many polyline vertices the payload holds, separators included.
    pub vertices: u32,
    /// Half the width of the polyline stroke.
    pub half_width: f32,
    /// The linear part of the map from payload to local space, as `[a, b, c, d]`:
    /// `x' = a·x + c·y` and `y' = b·x + d·y`.
    pub axes: [f32; 4],
}

impl MarkItem {
    /// An item over a payload of `counts` discs, boxes and vertices, filling `bounds` with
    /// `paint`.
    pub fn new(bounds: Rect<DevicePx, Device>, paint: PaintRef, counts: [u32; 3]) -> Self {
        Self {
            order: 0,
            flags: 0,
            bounds: [
                bounds.origin.x.0,
                bounds.origin.y.0,
                bounds.size.width.0,
                bounds.size.height.0,
            ],
            paint,
            clip: ClipId::ROOT.0,
            transform: SpatialId::VIEWPORT.index(),
            paint_origin: [0.0, 0.0],
            origin: [0.0, 0.0],
            discs: counts[0],
            boxes: counts[1],
            vertices: counts[2],
            half_width: 0.0,
            axes: [1.0, 0.0, 0.0, 1.0],
        }
    }

    /// The rectangle this paints, which is what draw order and culling are computed from.
    pub fn ink(&self) -> Rect<DevicePx, Device> {
        crate::prim::layout::rect_of(self.bounds)
    }

    /// The clip chain this draws through.
    pub fn clip_id(&self) -> ClipId {
        ClipId(self.clip)
    }

    /// Whether the item paints the union of its prims' coverage once.
    pub fn is_union(&self) -> bool {
        self.flags & MarkFlags::UNION != 0
    }

    /// Moves the item by `by`: its ink, its payload origin and its paint origin.
    ///
    /// The origin is in local space under every flag, so a move adds to it.
    pub fn reanchor(&mut self, by: Size<DevicePx, Device>) {
        self.bounds[0] += by.width.0;
        self.bounds[1] += by.height.0;
        self.origin[0] += by.width.0;
        self.origin[1] += by.height.0;
        self.paint_origin[0] += by.width.0;
        self.paint_origin[1] += by.height.0;
    }

    /// Whether the paint is read at the point being drawn rather than being one colour.
    pub fn samples_its_paint(&self) -> bool {
        self.paint.kind == PaintKind::Gradient as u32 || self.paint.kind == PaintKind::Image as u32
    }
}

/// One rounded, bordered rectangle of a payload.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct MarkBox {
    /// The outer edge, as `[x0, y0, x1, y1]`.
    pub rect: [f32; 4],
    /// Elliptical corner radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
    /// The superellipse exponent of the corners, the border width, and two unused values.
    pub shape: [f32; 4],
}

/// The geometry of one [`MarkItem`], shared between every copy of it.
///
/// No `PartialEq`: the polyline separators are NaN.
#[derive(Clone, Debug, Default)]
pub struct MarkPayload {
    /// Discs and rings, as `[cx, cy, outer radius, inner radius]`.
    pub discs: Vec<[f32; 4]>,
    /// Rectangles, rounded rectangles and their borders.
    pub boxes: Vec<MarkBox>,
    /// Polyline vertices. One NaN vertex separates two runs, and one NaN vertex comes first and
    /// last.
    pub vertices: Vec<[f32; 2]>,
}

impl MarkPayload {
    /// How many bytes the payload holds.
    pub fn bytes(&self) -> usize {
        self.discs.len() * size_of::<[f32; 4]>()
            + self.boxes.len() * size_of::<MarkBox>()
            + self.vertices.len() * size_of::<[f32; 2]>()
    }

    /// How many discs, boxes and vertices it holds.
    pub fn counts(&self) -> [u32; 3] {
        [
            self.discs.len() as u32,
            self.boxes.len() as u32,
            self.vertices.len() as u32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{DevicePx, Point, Rect, Size};

    use super::{MarkFlags, MarkItem};
    use crate::paint::PaintRef;

    #[test]
    fn a_new_mark_maps_its_payload_by_the_identity() {
        let bounds = Rect::new(
            Point::new(DevicePx(1.0), DevicePx(2.0)),
            Size::new(DevicePx(3.0), DevicePx(4.0)),
        );
        let mut mark = MarkItem::new(bounds, PaintRef::NONE, [1, 0, 0]);
        assert_eq!(mark.axes, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(mark.origin, [0.0, 0.0]);
        assert_eq!(mark.flags & (MarkFlags::SCREEN | MarkFlags::SQUARE_DISCS), 0);
        mark.reanchor(Size::new(DevicePx(5.0), DevicePx(-1.0)));
        assert_eq!(mark.origin, [5.0, -1.0]);
        assert_eq!(mark.axes, [1.0, 0.0, 0.0, 1.0], "a move keeps the axes");
    }
}
