//! One frame of the application, and what the display list said would be in it.

mod composed;

pub use composed::Tile;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui_render::FrameOutcome;
use zgui_render_wgpu::WgpuRenderer;
use zgui_scene::Scene;

use crate::device::picture::Picture;

/// Which primitive array a drawing arrived in.
///
/// The display list picks a route per shape. A shape it recognises arrives as quads or as one mark
/// item, a small one as a single-channel sprite, and only what is left as a vector item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A vector item, for the general rasteriser.
    Vector,
    /// A colour sprite, from the CPU layer route.
    Colour,
    /// A mark item: discs, boxes, polylines or path glyphs.
    Mark,
    /// A single-channel sprite: a shape rasterised as a coverage mask, or a glyph.
    Mask,
    /// A quad: a shape the analytic route recognised, or a box's background and border.
    Quad,
}

/// One drawing the display list held, and where it claimed its ink would land.
#[derive(Clone, Copy, Debug)]
pub struct Drawing {
    /// Its index in the array of its kind.
    pub order: usize,
    /// The array it is in.
    pub kind: Kind,
    /// What it paints, in device pixels, as the display list measured it.
    pub ink: Rect<DevicePx, Device>,
}

/// One filled rectangle the display list held.
///
/// The caret is one of these, and so is a border and a background — which is why the rectangle is
/// kept rather than a count: telling a caret from a field's own edge is a question about where and
/// how wide, and a fixture that counted quads would answer it for neither.
///
/// The rectangle is where the ink *lands*, which for a box under a transform is not the rectangle
/// the display list recorded. See [`placed`].
#[derive(Clone, Copy, Debug)]
pub struct Filled {
    /// Its place in the painting order.
    pub order: usize,
    /// Where it lands, in device pixels.
    pub bounds: Rect<DevicePx, Device>,
}

/// One glyph sprite the compositor drew.
///
/// The bounds are where the ink lands, with the sprite's transform applied.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// Where it lands, in device pixels.
    pub bounds: Rect<DevicePx, Device>,
    /// The atlas tile holding its coverage.
    pub tile: Tile,
}

/// What one of the application's frames produced.
///
/// A frame is read off the compositor's transcript and its vector plan.
pub struct Frame {
    /// Whether the frame drew nothing at all.
    pub blank: bool,
    /// The presented target, read back off the device.
    pub pixels: Picture,
    /// Every primitive only a drawing produces: each vector item, then each colour sprite, then
    /// each mark item.
    pub drawings: Vec<Drawing>,
    /// Every primitive a drawing can arrive as: the drawings, then each quad and each
    /// single-channel sprite. See [`Frame::shapes_in`].
    pub shapes: Vec<Drawing>,
    /// Every filled rectangle it held, in painting order.
    pub quads: Vec<Filled>,
    /// Every glyph it held, in painting order.
    pub glyphs: Vec<Glyph>,
    /// The persistent target frames are composed into, which is what a surface shows.
    pub composed: Picture,
    /// Each vector pass's region, its item range and whether it composites per item.
    pub pass_regions: Vec<(Rect<i32, Device>, core::ops::Range<usize>, bool)>,
}

impl Frame {
    /// Every shape whose middle is inside `rect`, without the quads that fill one of `boxes`.
    ///
    /// `boxes` are the element boxes of the document. A background or a border is a quad the size
    /// of its element's box, and a shape the analytic route drew is the size of the shape. A glyph
    /// that arrives as a single-channel sprite is still in the result, so a caller asks inside a
    /// rectangle with no text, or removes the text itself.
    pub fn shapes_in(
        &self,
        rect: Rect<DevicePx, Device>,
        boxes: &[Rect<DevicePx, Device>],
    ) -> Vec<Drawing> {
        self.shapes
            .iter()
            .copied()
            .filter(|shape| middle_inside(rect, shape.ink))
            .filter(|shape| {
                shape.kind != Kind::Quad || !boxes.iter().any(|held| same(*held, shape.ink))
            })
            .collect()
    }
}

/// Whether the middle of `inner` is inside `outer`.
fn middle_inside(outer: Rect<DevicePx, Device>, inner: Rect<DevicePx, Device>) -> bool {
    let x = inner.origin.x.0 + inner.size.width.0 / 2.0;
    let y = inner.origin.y.0 + inner.size.height.0 / 2.0;
    x >= outer.origin.x.0
        && x <= outer.origin.x.0 + outer.size.width.0
        && y >= outer.origin.y.0
        && y <= outer.origin.y.0 + outer.size.height.0
}

/// Whether two rectangles are the same to half a pixel on every edge.
fn same(left: Rect<DevicePx, Device>, right: Rect<DevicePx, Device>) -> bool {
    const SLACK: f32 = 0.5;
    (left.origin.x.0 - right.origin.x.0).abs() <= SLACK
        && (left.origin.y.0 - right.origin.y.0).abs() <= SLACK
        && (left.size.width.0 - right.size.width.0).abs() <= SLACK
        && (left.size.height.0 - right.size.height.0).abs() <= SLACK
}

/// One `[x, y, width, height]` as a rectangle.
fn rect_of(bounds: [f32; 4]) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(bounds[0]), DevicePx(bounds[1])),
        Size::new(DevicePx(bounds[2]), DevicePx(bounds[3])),
    )
}
