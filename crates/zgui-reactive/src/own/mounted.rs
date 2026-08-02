//! One owner per mounted node.

use reactive_graph::owner::Owner;

use crate::executor::{assert_ui_thread, note_owner_depth};
use crate::own::scope::Member;

/// The reactive scope of one mounted node, disposed of synchronously when the node unmounts.
///
/// Everything a node creates while it is being built — signals, memos, contexts, stored values,
/// cleanups, child nodes — belongs to this owner. [`unmount`](Mounted::unmount) disposes of all
/// of it at once, in one call, before it returns: child scopes first, then cleanups, then stored
/// values.
///
/// **Synchronously** is the load-bearing word. Letting the effects behind a node's bindings drop
/// their own scopes instead defers every cleanup by one executor poll, which means one frame in
/// which an unmounted node's cleanups have not run: a timer that fires into a disposed scope, a
/// row that still holds its slot in a shared table, an observer that reports geometry for a node
/// that no longer exists. Reading an arena-backed handle after its owner is disposed of panics,
/// so the frame in between is not a cosmetic problem.
///
/// ```
/// use zgui_reactive::{Mounted, RwSignal, install};
/// use zgui_reactive::prelude::*;
///
/// install().unwrap();
/// let node = Mounted::new();
/// let count = node.with(|| RwSignal::new(0));
///
/// count.set(1);
/// assert_eq!(count.get(), 1);
///
/// node.unmount(); // `count` is gone; reading it now would panic
/// ```
#[derive(Debug)]
#[must_use = "an unmounted scope frees nothing; store it and call `unmount` when the node goes away"]
pub struct Mounted {
    /// The node's owner.
    owner: Owner,
    /// The scope this node was mounted into, if any, told when the node goes away.
    membership: Option<Member>,
    /// Whether the owner has already been disposed of.
    disposed: bool,
}

impl Mounted {
    /// Creates a scope for a node mounted under the current one.
    ///
    /// With no current owner this is a root scope — what an application creates once, for its
    /// window. Anywhere else, call it while the parent's scope is current (inside
    /// [`Mounted::with`]) so the child is disposed of with its parent.
    ///
    /// # Panics
    ///
    /// In debug builds, if called off the UI thread.
    #[track_caller]
    pub fn new() -> Self {
        Self::from_owner(Owner::new(), None)
    }

    /// Wraps an owner, optionally as a member of a scope.
    #[track_caller]
    pub(crate) fn from_owner(owner: Owner, membership: Option<Member>) -> Self {
        assert_ui_thread("mounting a node");
        note_owner_depth(&owner);
        Self {
            owner,
            membership,
            disposed: false,
        }
    }

    /// Runs `build` with this node's scope current, and returns what it produced.
    ///
    /// Every reactive value `build` creates belongs to this node and dies with it. The previous
    /// scope is restored afterwards, including when `build` panics.
    pub fn with<T>(&self, build: impl FnOnce() -> T) -> T {
        self.owner.with(build)
    }

    /// This node's owner, for the few APIs that name one.
    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    /// Disposes of everything this node created, before returning.
    ///
    /// Child scopes are disposed of first, then cleanups run in reverse registration order,
    /// then stored values are dropped.
    pub fn unmount(mut self) {
        self.dispose();
    }

    /// The body of [`Mounted::unmount`], also used by the drop guard.
    ///
    /// A cleanup runs with **no scope current**, and deliberately so: a scope's cleanups also run
    /// when the last handle to its owner is dropped, which can happen anywhere, and a rule that
    /// held only on this path would be a rule nothing could rely on. Anything a cleanup has to
    /// reach must therefore have been captured when it was registered — which is why every
    /// release in this framework is a guard holding a handle rather than a lookup at the end.
    fn dispose(&mut self) {
        if !self.disposed {
            self.disposed = true;
            self.owner.cleanup();
            if let Some(membership) = self.membership.take() {
                membership.released();
            }
        }
    }
}

impl Default for Mounted {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Mounted {
    /// Disposes of the scope if [`Mounted::unmount`] was not called.
    ///
    /// A safety net, not the intended path: dropping a scope during a panic, or from a
    /// container that owns it, must still free it.
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::executor::install;
    use crate::own::on_cleanup_local;

    #[test]
    fn unmounting_a_parent_disposes_of_its_children() {
        install().unwrap();
        let calls = Rc::new(Cell::new(0));
        let parent = Mounted::new();

        let child = parent.with(Mounted::new);
        child.with({
            let calls = Rc::clone(&calls);
            move || on_cleanup_local(move || calls.set(calls.get() + 1))
        });
        std::mem::forget(child);

        parent.unmount();
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn dropping_a_scope_disposes_of_it_too() {
        install().unwrap();
        let calls = Rc::new(Cell::new(0));
        let node = Mounted::new();
        node.with({
            let calls = Rc::clone(&calls);
            move || on_cleanup_local(move || calls.set(calls.get() + 1))
        });

        drop(node);
        assert_eq!(calls.get(), 1);
    }
}
