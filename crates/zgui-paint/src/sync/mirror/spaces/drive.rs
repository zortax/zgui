//! How the compositor scrolls mirrored scroll nodes on its own.

use std::sync::Arc;

use compositor_types::{ScrollBehaviour, ScrollPhysics, SnapKind, Sources};

/// How the compositor moves a scroll node under gestures, without the client writing offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollDrive {
    /// Whether a wheel step glides and a released gesture flings, as opposed to applying at once.
    pub kinetic: bool,
    /// The input sources that stretch an end past itself, which then springs back.
    pub bounce: Sources,
    /// The input sources whose delta a node cannot take passes on to the node around it.
    pub chain: Sources,
}

/// What a scroll node's behaviour was last written from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Driven {
    /// The drive of every node.
    pub(super) drive: ScrollDrive,
    /// Whether gestures may scroll each axis, as `[x, y]`.
    pub(super) user: [bool; 2],
    /// Whether the node scrolls the content no gesture may scroll.
    pub(super) frozen: bool,
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
            bounce: self.drive.bounce,
            user_scrollable: self.user,
            chain: self.drive.chain,
        }
    }
}
