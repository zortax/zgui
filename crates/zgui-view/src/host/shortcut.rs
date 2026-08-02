//! The window-level key registration, and the guard that keeps it.

use core::fmt::{self, Debug};

use crate::host::handle::HostHandle;
use crate::id::NodeId;

/// Keeps a node hearing unfocused keys for as long as it is held.
///
/// Dropping it removes the registration, so the node goes back to hearing only the keys the path
/// to whatever holds focus runs through it.
///
/// A guard rather than a pair of calls for the reason every other guard here is one: the failure
/// mode of the pair is a registration outliving the view that made it, and after that the window
/// delivers keys to a node that is no longer in the document.
#[must_use = "dropping the guard removes the registration immediately"]
pub struct WindowShortcut {
    /// The host holding the registration.
    host: HostHandle,
    /// The node that is registered.
    node: NodeId,
}

impl WindowShortcut {
    /// Builds a guard over a registration that has already been made.
    ///
    /// Called by [`NodeRef::window_shortcut`](crate::NodeRef::window_shortcut); a component
    /// reaches for that rather than for this.
    pub fn new(host: HostHandle, node: NodeId) -> Self {
        Self { host, node }
    }

    /// Which node is registered.
    pub fn node(&self) -> NodeId {
        self.node
    }
}

impl Debug for WindowShortcut {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowShortcut")
            .field("node", &self.node)
            .finish_non_exhaustive()
    }
}

impl Drop for WindowShortcut {
    fn drop(&mut self) {
        self.host.remove_window_shortcut(self.node);
    }
}

#[cfg(test)]
mod tests {
    use super::WindowShortcut;
    use crate::host::handle::HostHandle;
    use crate::stub::StubHost;
    use crate::{DocumentId, NodeId};

    #[test]
    fn dropping_the_guard_removes_the_registration() {
        let stub = std::rc::Rc::new(StubHost::new());
        let host = HostHandle::from_rc(stub.clone());
        let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");

        host.add_window_shortcut(node);
        assert_eq!(stub.live_window_shortcuts(), 1);

        let guard = WindowShortcut::new(host.clone(), node);
        assert_eq!(guard.node(), node);
        drop(guard);

        assert_eq!(stub.live_window_shortcuts(), 0);
    }
}
