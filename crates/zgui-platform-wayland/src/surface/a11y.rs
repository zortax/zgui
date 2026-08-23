//! The channel an assistive technology talks to.
//!
//! # Why the tree is built by a closure and not passed in
//!
//! Building an accessibility tree costs a walk of the whole document. On a machine with no
//! assistive technology running — which is most machines, most of the time — that walk would be
//! pure waste on every frame, so the adapter is asked whether anything is listening and the walk
//! only happens if something is.
//!
//! # Why the adapter lives on the surface here
//!
//! On other desktops an adapter owns a window handle or a graph of platform objects and cannot
//! leave the thread that made it, so a backend has to keep it beside the loop and name it by
//! number. This desktop's adapter is a message channel to a bus connection on a thread of its own:
//! it is shareable, and every one of its handlers is documented as being called from another
//! thread. So it lives where it belongs — on the surface it speaks for — and a frame asking to
//! publish reaches it from wherever that frame ran.
//!
//! What the handlers must *not* do is answer. They run on the bus's thread, where the document is
//! not; so each of them turns into a wake, and the loop answers on the thread the document is on.

use std::sync::{Arc, Mutex};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use accesskit_unix::Adapter;
use zgui_platform::{SurfaceId, WakeReason, Waker};

/// One surface's accessibility channel.
#[derive(Default)]
pub struct A11y {
    /// The adapter, once one has been made.
    adapter: Mutex<Option<Adapter>>,
}

impl A11y {
    /// Opens a channel for the surface `id` names, answering through `waker`.
    pub fn open(id: SurfaceId, waker: Arc<dyn Waker>) -> Self {
        let adapter = Adapter::new(
            Activation {
                id,
                waker: Arc::clone(&waker),
            },
            Action {
                waker: Arc::clone(&waker),
            },
            Deactivation,
        );
        Self {
            adapter: Mutex::new(Some(adapter)),
        }
    }

    /// Publishes an update, building it only if something is listening.
    pub fn publish(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        if let Some(adapter) = self.lock().as_mut() {
            adapter.update_if_active(build);
        }
    }

    /// Tells the channel whether this window has the keyboard.
    ///
    /// A screen reader follows the focused window, so one that is never told stays pointed at
    /// whichever window was focused when it attached.
    pub fn focused(&self, focused: bool) {
        if let Some(adapter) = self.lock().as_mut() {
            adapter.update_window_focus_state(focused);
        }
    }

    /// Closes the channel, with the surface it spoke for.
    pub fn close(&self) {
        drop(self.lock().take());
    }

    /// The adapter, recovering from a panic on another thread.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Adapter>> {
        self.adapter
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl core::fmt::Debug for A11y {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("A11y")
            .field("attached", &self.lock().is_some())
            .finish()
    }
}

/// Something attached and wants a tree.
struct Activation {
    /// Which surface it wants one for.
    id: SurfaceId,
    /// How the loop is told.
    waker: Arc<dyn Waker>,
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // Nothing can be built here: this runs on the bus's thread and the document is on the
        // loop's. The contract has an answer for exactly this — a wake that forces a build even
        // when nothing is dirty, because the tree has never been produced and a dirty check has
        // nothing to notice.
        self.waker.wake(WakeReason::A11yTreeRequested(self.id));
        None
    }
}

/// Something asked for an action.
struct Action {
    /// How the loop is told.
    waker: Arc<dyn Waker>,
}

impl ActionHandler for Action {
    fn do_action(&mut self, request: ActionRequest) {
        // Queued rather than performed, which the trait documents as the preferred behaviour and
        // which is the only correct one here: performing it means dispatching into the document.
        self.waker.wake(WakeReason::A11yAction(request));
    }
}

/// Everything detached.
struct Deactivation;

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {
        // Nothing to drop and nothing to tell the application: what a deactivation means here is
        // that the next publish builds nothing, which is what `update_if_active` already does.
    }
}

#[cfg(test)]
mod tests {
    use super::A11y;
    use accesskit::{NodeId, Tree, TreeId, TreeUpdate};

    #[test]
    fn a_surface_with_no_channel_publishes_nothing_and_builds_nothing() {
        // The walk of the document is what publishing costs, so a window nothing is listening to
        // must not perform it — and a window with no channel at all certainly must not.
        let closed = A11y::default();
        let mut built = false;
        closed.publish(&mut || {
            built = true;
            unreachable!("the tree must not be walked when nothing is listening")
        });
        assert!(!built, "the tree was walked with nothing listening");
    }

    #[test]
    fn a_closed_channel_answers_every_later_request_with_nothing() {
        let closed = A11y::default();
        closed.focused(true);
        closed.close();
        let mut built = false;
        closed.publish(&mut || {
            built = true;
            TreeUpdate {
                nodes: Vec::new(),
                tree: Some(Tree::new(NodeId(0))),
                tree_id: TreeId::ROOT,
                focus: NodeId(0),
            }
        });
        assert!(!built);
    }
}
