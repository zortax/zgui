//! The content recorded around the ports of the nodes the compositor moves.
//!
//! While a node moves, every frame of the window damages a margin of one port's extent beyond each
//! scrollable edge, so the paint walk records the content there and keeps it current. The offsets
//! at which the port shows only that content are then vouched for. Past the margin's edge they
//! stop where the walk painted nothing, unless the margin reached the end of the content: a row
//! that does not exist yet is not recorded.

use compositor_types::ScrollId;
use zgui_geom::{Device, Matrix4, Point, Rect, Size};
use zgui_space::spatial::{Anchoring, PropertyOwner};
use zgui_space::SpatialId;

use crate::window::Window;

/// The margin of one moving node, measured for one frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Margin {
    /// The node.
    node: ScrollId,
    /// The coordinate system of the node's content.
    space: SpatialId,
    /// The port, as `[low, high]` per axis, in the content's coordinates at the frame's offset.
    port: [[f32; 2]; 2],
    /// The content recorded, as `[low, high]` per axis, in the content's coordinates.
    interest: [[f32; 2]; 2],
    /// The content's whole extent, as `[low, high]` per axis, in the content's coordinates.
    content: [[f32; 2]; 2],
    /// The offset the frame composes, in device pixels.
    offset: [f32; 2],
}

impl Window {
    /// Measures the margins of the nodes the compositor moves, and returns them with the device
    /// pixels of the parts outside their ports, which the frame damages.
    pub(crate) fn scroll_margins(&mut self) -> (Vec<Margin>, Vec<Rect<i32, Device>>) {
        let nodes: Vec<ScrollId> = self.driven.active.keys().copied().collect();
        let mut margins = Vec::new();
        let mut bands = Vec::new();
        for node in nodes {
            let Some(container) = self
                .painter
                .compositor()
                .and_then(|sync| sync.scroll_container(node))
            else {
                continue;
            };
            let Some((margin, world)) = self.margin_of(node, container) else {
                continue;
            };
            bands.extend(margin.bands().map(|band| onto_device(world, band)));
            margins.push(margin);
        }
        (margins, bands)
    }

    /// Records the offsets each of `margins` vouches for, from the extent the paint walk painted
    /// in each content's coordinates, `tracked`.
    pub(crate) fn vouch(
        &mut self,
        margins: &[Margin],
        tracked: &[(SpatialId, Rect<i32, Device>)],
    ) {
        for margin in margins {
            let painted = tracked
                .iter()
                .find(|(space, _)| *space == margin.space)
                .map(|(_, extent)| *extent);
            let vouched = [0, 1].map(|axis| margin.vouched(axis, painted));
            if let Some(active) = self.driven.active.get_mut(&margin.node) {
                active.vouched = Some(vouched);
            }
        }
    }

    /// Measures the margin of `node`, which scrolls the box `container`, and the world matrix of
    /// the port's coordinates.
    fn margin_of(
        &self,
        node: ScrollId,
        container: zgui_dom::side::BoxKey,
    ) -> Option<(Margin, Matrix4)> {
        let spatial = &self.tables.spatial;
        let space = spatial.of(PropertyOwner::of(container).content())?;
        let content = spatial.get(space)?;
        let Anchoring::Content(port) = content.anchoring else {
            return None;
        };
        let world = spatial.resolve(content.parent?)?;
        let offset = [-content.local.columns[3][0], -content.local.columns[3][1]];
        let origin = [port.rect.origin.x.0, port.rect.origin.y.0];
        let size = [port.rect.size.width.0, port.rect.size.height.0];
        let limit = [port.limit.width.0, port.limit.height.0];
        let user = port.user;
        let axes = [0, 1].map(|axis| {
            let low = origin[axis] + offset[axis];
            let shown = [low, low + size[axis]];
            let content = [origin[axis], origin[axis] + size[axis] + limit[axis]];
            let interest = if user[axis] && limit[axis] > 0.0 {
                [
                    (shown[0] - size[axis]).max(content[0]),
                    (shown[1] + size[axis]).min(content[1]),
                ]
            } else {
                shown
            };
            (shown, interest, content)
        });
        Some((
            Margin {
                node,
                space,
                port: axes.map(|(port, ..)| port),
                interest: axes.map(|(_, interest, _)| interest),
                content: axes.map(|(.., content)| content),
                offset,
            },
            world,
        ))
    }
}

impl Margin {
    /// The coordinate system of the node's content.
    pub(crate) fn space(&self) -> SpatialId {
        self.space
    }

    /// The parts of the margin outside the port, in the port's coordinates.
    fn bands(&self) -> impl Iterator<Item = [[f32; 2]; 2]> + '_ {
        let [x, y] = self.interest;
        let [port_x, port_y] = self.port;
        [
            [x, [y[0], port_y[0]]],
            [x, [port_y[1], y[1]]],
            [[x[0], port_x[0]], port_y],
            [[port_x[1], x[1]], port_y],
        ]
        .into_iter()
        .filter(|[x, y]| x[1] > x[0] && y[1] > y[0])
        .map(|[x, y]| {
            [
                [x[0] - self.offset[0], x[1] - self.offset[0]],
                [y[0] - self.offset[1], y[1] - self.offset[1]],
            ]
        })
    }

    /// The offsets on `axis` at which the port shows only recorded content, given the extent
    /// `painted` the walk painted in the content's coordinates.
    fn vouched(&self, axis: usize, painted: Option<Rect<i32, Device>>) -> [f32; 2] {
        let [low, high] = self.interest[axis];
        let [port_low, port_high] = self.port[axis];
        let [start, end] = self.content[axis];
        let painted = painted.map(|extent| match axis {
            0 => [extent.origin.x as f32, (extent.origin.x + extent.size.width) as f32],
            _ => [extent.origin.y as f32, (extent.origin.y + extent.size.height) as f32],
        });
        let painted_high = if high >= end {
            high
        } else {
            painted.map_or(port_high, |painted| painted[1].min(high).max(port_high))
        };
        let painted_low = if low <= start {
            low
        } else {
            painted.map_or(port_low, |painted| painted[0].max(low).min(port_low))
        };
        // The compositor keeps the offset inside the content, so a margin that reaches an end of
        // the content vouches for every offset past it.
        let shown = self.offset[axis];
        [
            if low <= start {
                f32::NEG_INFINITY
            } else {
                shown - (port_low - painted_low)
            },
            if high >= end {
                f32::INFINITY
            } else {
                shown + (painted_high - port_high)
            },
        ]
    }
}

/// Returns `rect`, given as `[low, high]` per axis in the coordinates `world` maps onto the
/// device, as the device pixels that cover it.
fn onto_device(world: Matrix4, [x, y]: [[f32; 2]; 2]) -> Rect<i32, Device> {
    let corners = [(x[0], y[0]), (x[1], y[0]), (x[0], y[1]), (x[1], y[1])].map(|(px, py)| {
        let c = world.columns;
        (
            c[0][0] * px + c[1][0] * py + c[3][0],
            c[0][1] * px + c[1][1] * py + c[3][1],
        )
    });
    let left = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min).floor();
    let top = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min).floor();
    let right = corners.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max).ceil();
    let bottom = corners.iter().map(|c| c.1).fold(f32::NEG_INFINITY, f32::max).ceil();
    Rect::new(
        Point::new(left as i32, top as i32),
        Size::new((right - left) as i32, (bottom - top) as i32),
    )
}
