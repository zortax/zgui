//! Focus traversal, and the guard that confines it.

use core::fmt::{self, Debug};

use crate::host::handle::HostHandle;
use crate::id::NodeId;

/// Which way focus should move.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum FocusMove {
    /// To the first focusable node in the subtree.
    First,
    /// To the last focusable node in the subtree.
    Last,
    /// To the next focusable node after the one that has focus now.
    Next,
    /// To the previous focusable node before the one that has focus now.
    Prev,
}

/// How a focus trap behaves while it is installed.
///
/// ```
/// use zgui_view::FocusTrapOptions;
///
/// // What a modal dialog wants: tab cycles inside, focus starts inside, and it goes back where
/// // it came from when the dialog closes.
/// let modal = FocusTrapOptions::default();
/// assert!(modal.wrap && modal.auto_focus && modal.restore);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub struct FocusTrapOptions {
    /// Whether moving past the last focusable node wraps to the first.
    pub wrap: bool,
    /// Whether focus moves into the subtree as the trap is installed.
    pub auto_focus: bool,
    /// Whether focus returns to whichever node held it when the trap was installed.
    pub restore: bool,
}

impl FocusTrapOptions {
    /// Cycling, self-focusing and restoring: what a modal surface wants.
    pub const MODAL: Self = Self {
        wrap: true,
        auto_focus: true,
        restore: true,
    };

    /// Confines traversal without moving focus in or out.
    ///
    /// What a non-modal surface wants — a menu opened from a toolbar, which keeps the toolbar's
    /// focus where it was.
    pub const CONFINE_ONLY: Self = Self {
        wrap: true,
        auto_focus: false,
        restore: false,
    };
}

impl Default for FocusTrapOptions {
    fn default() -> Self {
        Self::MODAL
    }
}

/// One installed focus trap, as the host named it.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct FocusTrapId(u64);

impl FocusTrapId {
    /// Wraps a host's own number.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The host's own number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Keeps a focus trap installed for as long as it is held.
///
/// Dropping it uninstalls the trap, and — if the trap was installed with
/// [`FocusTrapOptions::restore`] — puts focus back where it was. Traps stack, and the topmost one
/// wins, so a dialog opened from a dialog behaves.
///
/// This is a guard rather than a pair of calls because the failure mode of the pair is a window
/// that can never be tabbed out of again, and that failure survives every path that returns early.
#[must_use = "dropping the guard uninstalls the trap immediately"]
pub struct FocusTrap {
    /// The host that installed it, and can uninstall it.
    host: HostHandle,
    /// Which trap this is.
    id: FocusTrapId,
    /// The subtree it confines traversal to.
    root: NodeId,
}

impl FocusTrap {
    /// Builds a guard over an installed trap.
    ///
    /// Called by [`NodeRef::trap_focus`](crate::NodeRef::trap_focus); a component reaches for that
    /// rather than for this.
    pub fn new(host: HostHandle, root: NodeId, id: FocusTrapId) -> Self {
        Self { host, id, root }
    }

    /// Which trap this guard holds.
    pub fn id(&self) -> FocusTrapId {
        self.id
    }

    /// The subtree traversal is confined to.
    pub fn root(&self) -> NodeId {
        self.root
    }
}

impl Debug for FocusTrap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FocusTrap")
            .field("id", &self.id)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Drop for FocusTrap {
    fn drop(&mut self) {
        self.host.pop_focus_trap(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::{FocusTrap, FocusTrapOptions};
    use crate::host::handle::HostHandle;
    use crate::stub::StubHost;
    use crate::{DocumentId, NodeId};

    #[test]
    fn dropping_the_guard_uninstalls_the_trap() {
        let stub = std::rc::Rc::new(StubHost::new());
        let host = HostHandle::from_rc(stub.clone());
        let root = NodeId::new(DocumentId::FIRST, 1).expect("in range");

        let id = host.push_focus_trap(root, FocusTrapOptions::MODAL);
        assert_eq!(stub.live_focus_traps(), 1);

        let guard = FocusTrap::new(host.clone(), root, id);
        assert_eq!(guard.root(), root);
        drop(guard);

        assert_eq!(stub.live_focus_traps(), 0);
    }

    #[test]
    fn the_confining_options_move_no_focus() {
        let options = FocusTrapOptions::CONFINE_ONLY;
        assert!(options.wrap);
        assert!(!options.auto_focus);
        assert!(!options.restore);
    }
}
