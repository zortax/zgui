//! A listener a view attached to a node it did not create.

use core::fmt::{self, Debug};

use crate::dom::{DomHandle, ListenerId};
use crate::id::NodeId;

/// Keeps a listener attached for as long as it is held.
///
/// Dropping it removes the listener. It is a guard rather than a pair of calls because the thing
/// it is for — hearing about a press somewhere else in the window — is attached to a node the
/// view does not own and will not take out of the tree, so nothing else would ever remove it. An
/// overlay that leaked one would keep dismissing itself after it had gone.
///
/// Hold it in the component's own scope, or drop it deliberately to stop listening.
#[must_use = "dropping the guard removes the listener immediately"]
pub struct ListenerGuard {
    /// The tree it is attached in.
    dom: DomHandle,
    /// The node it is attached to.
    node: NodeId,
    /// Which registration it is.
    id: ListenerId,
}

impl ListenerGuard {
    /// Wraps a registration that has already been made.
    pub fn new(dom: DomHandle, node: NodeId, id: ListenerId) -> Self {
        Self { dom, node, id }
    }

    /// The node the listener is attached to.
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Which registration this guard holds.
    pub fn id(&self) -> ListenerId {
        self.id
    }
}

impl Debug for ListenerGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ListenerGuard")
            .field("node", &self.node)
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        self.dom.remove_listener(self.node, self.id);
    }
}
