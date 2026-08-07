//! One box's geometry, restated in this framework's own units.

use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Edges, Point, Rect, Size};

use crate::tree::store::LayoutStore;
use crate::tree::store::state::BoxLayout;

/// One box's resolved geometry, in device pixels and relative to its parent's border box.
///
/// This is the whole of what the layout algorithms produce, restated in this framework's own
/// units. Nothing downstream reads the engine's own result type, which is what lets the engine be
/// replaced.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResolvedLayout {
    /// Where the border box sits relative to the parent's border box.
    pub origin: Point<DevicePx, Device>,
    /// The border box's size.
    pub size: Size<DevicePx, Device>,
    /// How far the content reaches, which is what a scroll region is derived from.
    pub content_size: Size<DevicePx, Device>,
    /// Space reserved for scrollbars, on the axes that reserved it.
    pub scrollbar_size: Size<DevicePx, Device>,
    /// The resolved border widths.
    pub border: Edges<DevicePx>,
    /// The resolved padding widths.
    pub padding: Edges<DevicePx>,
    /// The resolved margins, which the parent has already consumed.
    pub margin: Edges<DevicePx>,
    /// This box's position among its siblings in painting order.
    pub order: u32,
    /// The first line box's baseline, measured down from the border-box top.
    pub first_baseline: Option<DevicePx>,
    /// The last line box's baseline, measured the same way.
    ///
    /// Carried beside the first because CSS aligns an `inline-block` in normal flow on its *last*
    /// line box, and a multi-line one would otherwise align on the wrong line.
    pub last_baseline: Option<DevicePx>,
}

impl ResolvedLayout {
    /// The border box, relative to the parent's.
    pub fn border_box(&self) -> Rect<DevicePx, Device> {
        Rect::new(self.origin, self.size)
    }

    /// The padding box, relative to the parent's border box.
    pub fn padding_box(&self) -> Rect<DevicePx, Device> {
        inset(self.border_box(), self.border)
    }

    /// The content box, relative to the parent's border box.
    ///
    /// The scrollbar gutter is taken off the same sides the layout engine took it off.
    pub fn content_box(&self) -> Rect<DevicePx, Device> {
        let padding = inset(self.padding_box(), self.padding);
        Rect::new(
            padding.origin,
            Size::new(
                DevicePx(padding.size.width.0 - self.scrollbar_size.width.0),
                DevicePx(padding.size.height.0 - self.scrollbar_size.height.0),
            ),
        )
    }
}

impl LayoutStore {
    /// One box's resolved geometry, or nothing if it has not been laid out.
    pub fn layout_of(&self, key: BoxKey) -> Option<ResolvedLayout> {
        let entry = self.layout.get(key)?.as_ref()?;
        Some(resolve(entry))
    }
}

/// Shrinks a rectangle by an inset on each side, never past zero.
fn inset(rect: Rect<DevicePx, Device>, edges: Edges<DevicePx>) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(
            DevicePx(rect.origin.x.0 + edges.left.0),
            DevicePx(rect.origin.y.0 + edges.top.0),
        ),
        Size::new(
            DevicePx((rect.size.width.0 - edges.left.0 - edges.right.0).max(0.0)),
            DevicePx((rect.size.height.0 - edges.top.0 - edges.bottom.0).max(0.0)),
        ),
    )
}

/// Restates the engine's result in this framework's own units.
fn resolve(entry: &BoxLayout) -> ResolvedLayout {
    let layout = &entry.snapped;
    ResolvedLayout {
        origin: Point::new(DevicePx(layout.location.x), DevicePx(layout.location.y)),
        size: Size::new(DevicePx(layout.size.width), DevicePx(layout.size.height)),
        content_size: Size::new(
            DevicePx(layout.content_size.width),
            DevicePx(layout.content_size.height),
        ),
        scrollbar_size: Size::new(
            DevicePx(layout.scrollbar_size.width),
            DevicePx(layout.scrollbar_size.height),
        ),
        border: edges(layout.border),
        padding: edges(layout.padding),
        margin: edges(layout.margin),
        order: layout.order,
        first_baseline: entry.first_baseline.map(DevicePx),
        last_baseline: entry.last_baseline.map(DevicePx),
    }
}

/// Restates the engine's four sides in this framework's own units.
fn edges(rect: taffy::Rect<f32>) -> Edges<DevicePx> {
    Edges {
        top: DevicePx(rect.top),
        right: DevicePx(rect.right),
        bottom: DevicePx(rect.bottom),
        left: DevicePx(rect.left),
    }
}
