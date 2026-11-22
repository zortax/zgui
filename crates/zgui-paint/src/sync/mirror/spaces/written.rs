//! A coordinate system's value in the compositor's vocabulary.

use compositor_types::{
    Anchor, Matrix4, ScrollBehaviour, ScrollId, ScrollNode, StickyAnchor, TransformId,
    TransformNode,
};
use zgui_space::spatial::{Anchoring, ScrollPort, SpatialNode, StickyPort};

/// What a mirrored transform node and its scroll node were last written with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Written {
    /// The parent node.
    pub(crate) parent: Option<TransformId>,
    /// The local matrix.
    pub(crate) local: Matrix4,
    /// Whether the node takes none of the scroll offsets above it.
    pub(crate) fixed: bool,
    /// The scroll node the node applies, for the content of a scroll container.
    pub(crate) scroll: Option<Scroll>,
    /// What holds the node inside a scroll port, for a sticky box drawn in a scroll container's
    /// content.
    pub(crate) sticky: Option<Sticky>,
}

/// The scroll node a sticky node sticks to, and what holds it there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sticky {
    /// The scroll node whose port the node sticks to.
    pub(crate) scroll: ScrollId,
    /// The box where the flow puts it, in the scroll node's space at zero offset.
    pub(crate) rect: [f32; 4],
    /// The rectangle it stays inside, in the same space.
    pub(crate) limit: [f32; 4],
    /// The insets from the port's edges, as `[top, right, bottom, left]`, `None` for a free edge.
    pub(crate) inset: [Option<f32>; 4],
}

/// The port, content size and offset of a scroll node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Scroll {
    /// The visible rectangle, in the parent node's space.
    pub(crate) port: [f32; 4],
    /// The size of the content.
    pub(crate) content: [f32; 2],
    /// The offset, within the range `content` allows.
    pub(crate) offset: [f32; 2],
    /// Whether a person may scroll each axis, as `[x, y]`.
    pub(crate) user: [bool; 2],
}

impl Written {
    /// Returns the value of `node`, drawn under `parent`, or the identity root for no node.
    ///
    /// The content of a scroll container applies its offset through a scroll node. The part of
    /// the offset past the scroll range stays in the local matrix. A sticky box drawn in the
    /// content of a scroll container, whose node applies the scroll node `parent_scroll`, sticks to
    /// that scroll node, and its shift is left out of the local matrix.
    pub(crate) fn of(
        node: Option<&SpatialNode>,
        parent: Option<TransformId>,
        parent_scroll: Option<ScrollId>,
    ) -> Self {
        let Some(node) = node else {
            return Self {
                parent: None,
                local: Matrix4::IDENTITY,
                fixed: false,
                scroll: None,
                sticky: None,
            };
        };
        let local = Matrix4 {
            columns: node.local.columns,
        };
        match node.anchoring {
            Anchoring::Content(port) if parent.is_some() => content(port, local, parent),
            Anchoring::Fixed => Self {
                parent,
                local,
                fixed: true,
                scroll: None,
                sticky: None,
            },
            Anchoring::Sticky(held) if parent_scroll.is_some() => {
                sticky(held, local, parent, parent_scroll)
            }
            _ => Self {
                parent,
                local,
                fixed: false,
                scroll: None,
                sticky: None,
            },
        }
    }

    /// Returns the transform node, applying `scroll`.
    pub(crate) fn node(&self, scroll: Option<ScrollId>) -> TransformNode {
        let anchor = match self.sticky {
            _ if self.fixed => Anchor::Fixed,
            Some(sticky) => Anchor::Sticky(StickyAnchor {
                scroll: sticky.scroll,
                rect: sticky.rect,
                limit: sticky.limit,
                inset: sticky.inset.map(|edge| edge.unwrap_or(f32::NAN)),
            }),
            None => Anchor::Scrolls,
        };
        TransformNode {
            parent: self.parent,
            local: self.local,
            scroll,
            anchor,
        }
    }
}

impl Scroll {
    /// Returns the scroll node measured in `space`, which the compositor scrolls with `behaviour`.
    pub(crate) fn node(
        &self,
        space: TransformId,
        behaviour: Option<ScrollBehaviour>,
    ) -> ScrollNode {
        ScrollNode {
            space,
            port: self.port,
            content: self.content,
            offset: self.offset,
            behaviour,
        }
    }

    /// Reports whether `next` differs in more than the offset.
    pub(crate) fn reshaped(&self, next: &Self) -> bool {
        self.port != next.port || self.content != next.content
    }
}

/// Returns the value of a sticky box drawn in the content of a scroll container, whose node applies
/// `scroll`, with the shift its local matrix holds left out.
fn sticky(
    held: StickyPort,
    local: Matrix4,
    parent: Option<TransformId>,
    scroll: Option<ScrollId>,
) -> Written {
    let mut local = local;
    local.columns[3][0] -= held.shift.0 * local.columns[3][3];
    local.columns[3][1] -= held.shift.1 * local.columns[3][3];
    let rect = |rect: zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>| {
        [
            rect.origin.x.0,
            rect.origin.y.0,
            rect.size.width.0,
            rect.size.height.0,
        ]
    };
    Written {
        parent,
        local,
        fixed: false,
        scroll: None,
        sticky: scroll.map(|scroll| Sticky {
            scroll,
            rect: rect(held.rect),
            limit: rect(held.limit),
            inset: held.inset,
        }),
    }
}

/// Returns the value of a scroll container's content, whose local matrix translates by the
/// negated offset.
fn content(port: ScrollPort, local: Matrix4, parent: Option<TransformId>) -> Written {
    let [x, y, ..] = local.columns[3];
    let wanted = [-x, -y];
    let limit = [port.limit.width.0, port.limit.height.0];
    let offset = [0, 1].map(|axis| wanted[axis].clamp(0.0, limit[axis].max(0.0)));
    Written {
        parent,
        local: Matrix4::translation(offset[0] - wanted[0], offset[1] - wanted[1], 0.0),
        fixed: false,
        sticky: None,
        scroll: Some(Scroll {
            port: [
                port.rect.origin.x.0,
                port.rect.origin.y.0,
                port.rect.size.width.0,
                port.rect.size.height.0,
            ],
            content: [
                port.rect.size.width.0 + limit[0].max(0.0),
                port.rect.size.height.0 + limit[1].max(0.0),
            ],
            offset,
            user: port.user,
        }),
    }
}
