//! A coordinate system's value in the compositor's vocabulary.

use compositor_types::{Anchor, Matrix4, ScrollId, ScrollNode, TransformId, TransformNode};
use zgui_scene::spatial::{Anchoring, ScrollPort, SpatialNode};

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
}

impl Written {
    /// Returns the value of `node`, drawn under `parent`, or the identity root for no node.
    ///
    /// The content of a scroll container applies its offset through a scroll node. The part of
    /// the offset past the scroll range stays in the local matrix.
    pub(crate) fn of(node: Option<&SpatialNode>, parent: Option<TransformId>) -> Self {
        let Some(node) = node else {
            return Self {
                parent: None,
                local: Matrix4::IDENTITY,
                fixed: false,
                scroll: None,
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
            },
            // A sticky box is composed against its port, so its offset is in its own geometry.
            _ => Self {
                parent,
                local,
                fixed: false,
                scroll: None,
            },
        }
    }

    /// Returns the transform node, applying `scroll`.
    pub(crate) fn node(&self, scroll: Option<ScrollId>) -> TransformNode {
        TransformNode {
            parent: self.parent,
            local: self.local,
            scroll,
            anchor: if self.fixed {
                Anchor::Fixed
            } else {
                Anchor::Scrolls
            },
        }
    }
}

impl Scroll {
    /// Returns the scroll node measured in `space`.
    pub(crate) fn node(&self, space: TransformId) -> ScrollNode {
        ScrollNode {
            space,
            port: self.port,
            content: self.content,
            offset: self.offset,
            behaviour: None,
        }
    }

    /// Reports whether `next` differs in more than the offset.
    pub(crate) fn reshaped(&self, next: &Self) -> bool {
        self.port != next.port || self.content != next.content
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
        }),
    }
}
