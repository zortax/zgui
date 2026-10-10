//! What one draw of a mark is told, and which of its payload kinds it walks.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Device, Point, Rect};

/// One payload kind of a mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MarkKind {
    /// Discs and rings.
    Disc,
    /// Rectangles, rounded rectangles and their borders.
    Box,
    /// Stroked polylines.
    Polyline,
    /// Copies of repeated outlines, read from atlas cells.
    Glyph,
}

impl MarkKind {
    /// Every kind, in payload-lane order.
    pub const ALL: [Self; 4] = [Self::Disc, Self::Box, Self::Polyline, Self::Glyph];

    /// The kind's payload lane.
    pub fn lane(self) -> usize {
        self as usize
    }
}

/// The block one draw of one mark item reads.
///
/// One per item and frame: the coverage draws and the composite of a union item read the same
/// block, and a paint draw reads one whose bin fields are unused.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct MarkDraw {
    /// The item's position in the draw-order remap.
    pub position: u32,
    /// The bin page a coverage draw writes and the composite reads.
    pub page: u32,
    /// Added to a device point to find its texel in the page.
    pub shift: [f32; 2],
    /// The device pixels the bin holds: origin then extent.
    pub region: [f32; 4],
}

impl MarkDraw {
    /// The block of a paint draw, which reads no bin.
    pub fn direct(position: u32) -> Self {
        Self {
            position,
            ..Self::default()
        }
    }

    /// The block of an item binned at `at` of `page`, holding `region`.
    pub fn binned(
        position: u32,
        page: u32,
        region: Rect<i32, Device>,
        at: Point<i32, Device>,
    ) -> Self {
        Self {
            position,
            page,
            shift: [
                (at.x - region.origin.x) as f32,
                (at.y - region.origin.y) as f32,
            ],
            region: [
                region.origin.x as f32,
                region.origin.y as f32,
                region.size.width as f32,
                region.size.height as f32,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MarkDraw;
    use zgui_geom::{Point, Rect, Size};

    #[test]
    fn a_bin_shift_takes_the_region_origin_to_its_place_in_the_page() {
        let draw = MarkDraw::binned(
            3,
            1,
            Rect::new(Point::new(40, 50), Size::new(10, 20)),
            Point::new(100, 0),
        );
        assert_eq!(draw.shift, [60.0, -50.0]);
        assert_eq!(draw.region, [40.0, 50.0, 10.0, 20.0]);
        assert_eq!(draw.page, 1);
    }
}
