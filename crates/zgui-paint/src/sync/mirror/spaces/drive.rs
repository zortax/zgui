//! How the compositor scrolls mirrored scroll nodes on its own.

use std::sync::Arc;

use compositor_types::{Overscroll, ScrollBehaviour, ScrollPhysics, SnapKind};

/// How the compositor moves a scroll node under gestures, without the client writing offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollDrive {
    /// Whether a wheel step glides and a released gesture flings, as opposed to applying at once.
    pub kinetic: bool,
    /// Whether an end stretches past itself and springs back, as opposed to stopping the content.
    pub bounce: bool,
}

/// What a scroll node's behaviour was last written from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Driven {
    /// The drive of every node.
    pub(super) drive: ScrollDrive,
    /// Whether gestures may scroll each axis, as `[x, y]`.
    pub(super) user: [bool; 2],
}

impl Driven {
    /// Returns the behaviour the compositor scrolls the node with.
    pub(super) fn behaviour(self) -> ScrollBehaviour {
        ScrollBehaviour {
            physics: if self.drive.kinetic {
                ScrollPhysics::Kinetic
            } else {
                ScrollPhysics::Instant
            },
            snap: Arc::new([]),
            snap_kind: SnapKind::Mandatory,
            overscroll: if self.drive.bounce {
                Overscroll::Bounce
            } else {
                Overscroll::Clamp
            },
            user_scrollable: self.user,
            chaining: true,
        }
    }
}
