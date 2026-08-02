//! One coordinate system, and how it relates to the scrolling above it.

use zgui_geom::Matrix4;

use crate::id::ScrollFrameId;
use crate::spatial::id::SpatialId;
use crate::spatial::tree::PropertyNode;

/// A node of the spatial tree: one coordinate system.
///
/// Its name is the box's, so writing `local` sixty times a second moves the coordinate system
/// without ever renaming it. That is the whole difference between this and interning a matrix,
/// where the value *is* the name and every distinct matrix is a distinct identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialNode {
    /// The coordinate system this one is expressed in, or `None` for a root.
    pub parent: Option<SpatialId>,
    /// This node's own transform within its parent.
    pub local: Matrix4,
    /// How it takes the scroll offsets above it.
    pub anchoring: Anchoring,
}

impl PropertyNode<SpatialId> for SpatialNode {
    fn parent(&self) -> Option<SpatialId> {
        self.parent
    }
}

/// How a coordinate system relates to the scrolling above it.
///
/// Recorded when the node is established rather than looked for afterwards, because the styles it
/// is read from are in front of the code that establishes the node and behind the code that
/// resolves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchoring {
    /// Moves with every scroll offset above it. The overwhelming majority.
    Scrolling,
    /// Its shift is measured against a scrollport it does not travel with, so it holds still while
    /// its neighbours slide past.
    Sticky {
        /// The scrollable region the shift is measured against.
        port: ScrollFrameId,
    },
    /// Takes none of the scroll offsets above it, so it does not move at all when they change.
    Fixed,
}

/// What a box establishes of its own, when it is not simply drawn in the space above it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnSpace {
    /// The box's own transform within the space above it.
    pub local: Matrix4,
    /// How it takes the scroll offsets above it.
    pub anchoring: Anchoring,
}

impl OwnSpace {
    /// What a box establishes, from the three things that decide whether every piece at and below
    /// it moves by the same vector as the box above it.
    ///
    /// Those three are the whole of the answer, and they are the same three either way round: a
    /// box establishes a coordinate system of its own exactly when it is *not* one that moves by
    /// the vector its parent moved by. A transformed box's matrix is composed against its own
    /// border box; a sticky box's shift is measured against a scrollport it does not travel with;
    /// a box positioned against the viewport takes none of the scroll offsets above it. Everything
    /// else — which is nearly everything — answers `None` and shares the space it is drawn in.
    ///
    /// Sticky and viewport anchoring are two values of one property and never both apply.
    ///
    /// ```
    /// use zgui_geom::Matrix4;
    /// use zgui_scene::{Anchoring, OwnSpace, ScrollFrameId};
    ///
    /// assert_eq!(OwnSpace::of(None, None, false), None, "drawn in the space above it");
    ///
    /// let moved = Matrix4::translation(4.0, 0.0, 0.0);
    /// assert_eq!(
    ///     OwnSpace::of(Some(moved), None, false),
    ///     Some(OwnSpace { local: moved, anchoring: Anchoring::Scrolling }),
    ///     "a transform scrolls like anything else; what it establishes is the space, not the anchor",
    /// );
    /// assert_eq!(
    ///     OwnSpace::of(None, None, true).map(|own| own.anchoring),
    ///     Some(Anchoring::Fixed),
    /// );
    /// assert_eq!(
    ///     OwnSpace::of(None, Some(ScrollFrameId(3)), false).map(|own| own.anchoring),
    ///     Some(Anchoring::Sticky { port: ScrollFrameId(3) }),
    /// );
    /// ```
    pub fn of(
        matrix: Option<Matrix4>,
        sticky_port: Option<ScrollFrameId>,
        ignores_scroll: bool,
    ) -> Option<Self> {
        let anchoring = match (sticky_port, ignores_scroll) {
            (Some(port), _) => Anchoring::Sticky { port },
            (None, true) => Anchoring::Fixed,
            (None, false) => Anchoring::Scrolling,
        };
        if matrix.is_none() && anchoring == Anchoring::Scrolling {
            return None;
        }
        Some(Self {
            local: matrix.unwrap_or(Matrix4::IDENTITY),
            anchoring,
        })
    }
}
