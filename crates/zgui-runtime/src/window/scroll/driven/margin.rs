//! The content recorded around the ports of the nodes the compositor moves.
//!
//! While a node moves, the window records a margin of one port's extent beyond the port's edges
//! ahead of the motion. Each frame damages the part of the margin the last frame did not record,
//! and content that changes inside the margin damages itself as it would inside the port, so the
//! recorded content stays current.
//!
//! The offsets at which the port shows only recorded content are then vouched for. They stop where
//! the walk painted nothing, because a row that does not exist yet is not recorded. A margin that
//! reaches an end of the content vouches for every offset past it. For content built in steps,
//! such as the rows of a virtual list, that holds only once its paint reaches the end.

use iceblink::smallvec::SmallVec;
use iceblink::{OffsetRange, ScrollId, ScrollWatch};
use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};
use zgui_space::SpatialId;
use zgui_space::spatial::{Anchoring, PropertyOwner};

use crate::window::Window;

/// A rectangle given as `[low, high]` per axis.
pub(super) type Span = [[f32; 2]; 2];

/// The margins of the nodes the compositor moves, measured for one frame.
#[derive(Debug, Default)]
pub(crate) struct Margins {
    /// Each node's margin.
    pub(crate) measured: Vec<Margin>,
    /// The device pixels of the margins outside their ports, which the frame's damage is cut to.
    pub(crate) bands: Vec<Rect<i32, Device>>,
    /// The same without the content the window's last frame recorded, which the frame damages.
    pub(crate) fresh: Vec<Rect<i32, Device>>,
}

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
    /// Whether the content is built in steps as the port moves, so content not built yet paints
    /// nothing.
    stepped: bool,
}

impl Window {
    /// Measures the margins of the nodes the compositor moves for the frame being painted.
    pub(crate) fn scroll_margins(&mut self) -> Margins {
        let nodes: Vec<(ScrollId, [f32; 2], Option<Span>)> = self
            .driven
            .active
            .iter()
            .map(|(node, active)| (*node, active.step, active.recorded))
            .collect();
        let mut margins = Margins::default();
        for (node, step, recorded) in nodes {
            let Some(container) = self
                .painter
                .compositor()
                .and_then(|sync| sync.scroll_container(node))
            else {
                continue;
            };
            let Some((mut margin, world)) = self.margin_of(node, container, step) else {
                continue;
            };
            margin.stepped = self
                .element_of_scroll(node)
                .is_some_and(|element| self.host.held_offsets(element).is_some());
            let device = |band| onto_device(world, band);
            margins.bands.extend(margin.bands(None).map(device));
            margins.fresh.extend(margin.bands(recorded).map(device));
            if let Some(active) = self.driven.active.get_mut(&node) {
                active.recorded = Some(margin.interest);
                active.reach = Some(device(margin.shown(margin.interest)));
            }
            margins.measured.push(margin);
        }
        margins
    }

    /// Widens the clips of the nodes the compositor moves to the margins the window's last frame
    /// recorded, and returns the device pixels those margins and their ports cover.
    ///
    /// Content that changes in a margin is then damaged as content the port shows is.
    pub(crate) fn widen_moving_ports(&mut self) -> Option<Rect<i32, Device>> {
        let ports: Vec<_> = self
            .driven
            .active
            .iter()
            .filter_map(|(node, active)| {
                let container = self
                    .painter
                    .compositor_ref()
                    .and_then(|sync| sync.scroll_container(*node))?;
                Some((container, active.reach?))
            })
            .collect();
        let reach = ports.iter().map(|(_, reach)| *reach).reduce(Rect::union);
        self.diff_scratch.widen_ports(
            ports
                .into_iter()
                .map(|(container, reach)| (container, device_px(reach))),
        );
        reach
    }

    /// Records the offsets each of `margins` vouches for, from the extent the recorded chunks of
    /// each content draw over, and watches the motion of each node that leaves them.
    pub(crate) fn vouch(&mut self, margins: &[Margin]) {
        for margin in margins {
            let painted = self
                .painter
                .compositor_ref()
                .and_then(|sync| sync.recorded_extent(margin.space));
            let vouched = [0, 1].map(|axis| margin.vouched(axis, painted));
            if let Some(active) = self.driven.active.get_mut(&margin.node) {
                active.vouched = Some(vouched);
            }
            self.watch_motion(margin.node, vouched);
        }
    }

    /// Sets the watch of the scroll node `node`, whose port shows only recorded content at the
    /// offsets `vouched`.
    ///
    /// The watch wakes the window in these cases:
    ///
    /// - The node comes to rest.
    /// - Its offset two motion steps ahead leaves `vouched`.
    /// - Its offset one step ahead leaves the offsets its container's rows hold, see
    ///   [`ViewHost::wake_outside`](zgui_view::ViewHost::wake_outside).
    /// - It moves the element under the pointer out from under it.
    pub(crate) fn watch_motion(&mut self, node: ScrollId, vouched: Span) {
        let mut ranges = SmallVec::new();
        ranges.push(OffsetRange {
            bounds: vouched,
            ahead: 2.0,
        });
        let held = self
            .element_of_scroll(node)
            .and_then(|container| self.host.held_offsets(container));
        if let Some([low, high]) = held {
            // The rows hold `[low, high)` in CSS pixels, and a range holds its bounds.
            ranges.push(OffsetRange {
                bounds: [
                    [f32::NEG_INFINITY, f32::INFINITY],
                    [low * self.scale, (high * self.scale).next_down()],
                ],
                ahead: 1.0,
            });
        }
        let hover = self.hover_range(node).map(|range| {
            ranges.push(range);
            ranges.len() - 1
        });
        if let Some(active) = self.driven.active.get_mut(&node) {
            active.hover = hover;
        }
        self.compositor.watch_scroll(
            node,
            Some(ScrollWatch {
                ranges,
                stride: [0.0; 2],
                settle: true,
            }),
        );
    }

    /// Measures the margin of `node`, which scrolls the box `container` and moved by `step` in its
    /// last frame, and the world matrix of the port's coordinates.
    ///
    /// The margin lies ahead of the motion on an axis that moves, and on both sides of one that
    /// does not.
    fn margin_of(
        &self,
        node: ScrollId,
        container: zgui_dom::side::BoxKey,
        step: [f32; 2],
    ) -> Option<(Margin, Matrix4)> {
        let spatial = &self.tables.spatial;
        let space = spatial.of(PropertyOwner::of(container).content())?;
        let content = spatial.get(space)?;
        let Anchoring::Content(port) = content.anchoring else {
            return None;
        };
        // The content of the document's own scroll container hangs from the viewport.
        let world = match content.parent {
            Some(parent) => spatial.resolve(parent)?,
            None => Matrix4::IDENTITY,
        };
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
                let before = if step[axis] > 0.0 { 0.0 } else { size[axis] };
                let after = if step[axis] < 0.0 { 0.0 } else { size[axis] };
                [
                    (shown[0] - before).max(content[0]),
                    (shown[1] + after).min(content[1]),
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
                stepped: false,
            },
            world,
        ))
    }
}

impl Margin {
    /// The parts of the margin outside the port and outside `recorded`, in the port's
    /// coordinates.
    fn bands(&self, recorded: Option<Span>) -> impl Iterator<Item = Span> + '_ {
        let [x, y] = self.interest;
        let [port_x, port_y] = self.port;
        [
            [x, [y[0], port_y[0]]],
            [x, [port_y[1], y[1]]],
            [[x[0], port_x[0]], port_y],
            [[port_x[1], x[1]], port_y],
        ]
        .into_iter()
        .flat_map(move |band| subtract(band, recorded))
        .filter(|[x, y]| x[1] > x[0] && y[1] > y[0])
        .map(|span| self.shown(span))
    }

    /// Returns `span`, given in the content's coordinates, in the port's coordinates.
    fn shown(&self, [x, y]: Span) -> Span {
        [
            [x[0] - self.offset[0], x[1] - self.offset[0]],
            [y[0] - self.offset[1], y[1] - self.offset[1]],
        ]
    }

    /// The offsets on `axis` at which the port shows only recorded content, given the extent
    /// `painted` the walk painted in the content's coordinates.
    fn vouched(&self, axis: usize, painted: Option<Rect<i32, Device>>) -> [f32; 2] {
        let [low, high] = self.interest[axis];
        let [port_low, port_high] = self.port[axis];
        let [start, end] = self.content[axis];
        let painted = painted.map(|extent| match axis {
            0 => [
                extent.origin.x as f32,
                (extent.origin.x + extent.size.width) as f32,
            ],
            _ => [
                extent.origin.y as f32,
                (extent.origin.y + extent.size.height) as f32,
            ],
        });
        let painted_high = painted.map_or(port_high, |painted| painted[1].min(high).max(port_high));
        let painted_low = painted.map_or(port_low, |painted| painted[0].max(low).min(port_low));
        // The compositor keeps the offset inside the content, so a margin that reaches an end of
        // the content vouches for every offset past it. Content built in steps has to paint up to
        // the end first.
        let (reaches_low, reaches_high) = if self.stepped {
            (painted_low <= start + 1.0, painted_high >= end - 1.0)
        } else {
            (low <= start, high >= end)
        };
        let shown = self.offset[axis];
        [
            if reaches_low {
                f32::NEG_INFINITY
            } else {
                shown - (port_low - painted_low)
            },
            if reaches_high {
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
    let left = corners
        .iter()
        .map(|c| c.0)
        .fold(f32::INFINITY, f32::min)
        .floor();
    let top = corners
        .iter()
        .map(|c| c.1)
        .fold(f32::INFINITY, f32::min)
        .floor();
    let right = corners
        .iter()
        .map(|c| c.0)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil();
    let bottom = corners
        .iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil();
    Rect::new(
        Point::new(left as i32, top as i32),
        Size::new((right - left) as i32, (bottom - top) as i32),
    )
}

/// Returns the parts of `rect` outside `cut`, both given as `[low, high]` per axis, some of them
/// empty.
fn subtract(rect: Span, cut: Option<Span>) -> [Span; 4] {
    let empty = [[0.0; 2]; 2];
    let Some([cx, cy]) = cut else {
        return [rect, empty, empty, empty];
    };
    let [x, y] = rect;
    let inner_x = [x[0].max(cx[0]), x[1].min(cx[1])];
    let inner_y = [y[0].max(cy[0]), y[1].min(cy[1])];
    if inner_x[1] <= inner_x[0] || inner_y[1] <= inner_y[0] {
        return [rect, empty, empty, empty];
    }
    [
        [x, [y[0], inner_y[0]]],
        [x, [inner_y[1], y[1]]],
        [[x[0], inner_x[0]], inner_y],
        [[inner_x[1], x[1]], inner_y],
    ]
}

/// Returns `rect` in the device pixels a clip is measured in.
fn device_px(rect: Rect<i32, Device>) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(
            DevicePx(rect.origin.x as f32),
            DevicePx(rect.origin.y as f32),
        ),
        Size::new(
            DevicePx(rect.size.width as f32),
            DevicePx(rect.size.height as f32),
        ),
    )
}
