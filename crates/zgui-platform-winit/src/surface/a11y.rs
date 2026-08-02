//! The accessibility channel one window talks to.

use std::sync::Mutex;

use accesskit::TreeUpdate;
use accesskit_winit::Adapter;
use winit::event::WindowEvent;
use winit::window::Window;

/// One window's accessibility adapter, or nothing where none could be attached.
///
/// # Why the tree is built by a closure and not passed in
///
/// Building an accessibility tree costs a walk of the whole document. On a machine with no
/// assistive technology running — which is most machines, most of the time — that walk would be
/// pure waste on every frame, so the adapter is asked whether anything is listening and the walk
/// only happens if something is.
///
/// # Why it is behind a lock
///
/// A surface is shared and usable from any thread, because a frame can be asked for from any
/// thread. The adapter is not, so it is kept behind a lock that is taken for the length of one
/// update. Nothing else contends for it: updates are published from the frame's last phase, one
/// window at a time.
#[derive(Default)]
pub(crate) struct A11y {
    /// The adapter, once one has been attached.
    adapter: Mutex<Option<Adapter>>,
}

impl A11y {
    /// Holds `adapter` for this window.
    pub(crate) fn attach(&self, adapter: Adapter) {
        *self.adapter.lock().expect("the adapter is not poisoned") = Some(adapter);
    }

    /// Whether anything was ever attached.
    pub(crate) fn is_attached(&self) -> bool {
        self.adapter
            .lock()
            .expect("the adapter is not poisoned")
            .is_some()
    }

    /// Publishes an update, building it only if something is listening.
    pub(crate) fn publish(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        if let Some(adapter) = self
            .adapter
            .lock()
            .expect("the adapter is not poisoned")
            .as_mut()
        {
            adapter.update_if_active(build);
        }
    }

    /// Lets the adapter see a window event before anything else does.
    ///
    /// The adapter tracks where the window is and whether it has focus, and it learns both only
    /// from the events it is shown. One not shown to it reports the window's bounds from before
    /// the move, which is what puts a screen reader's highlight in the wrong place.
    pub(crate) fn observe(&self, window: &Window, event: &WindowEvent) {
        if let Some(adapter) = self
            .adapter
            .lock()
            .expect("the adapter is not poisoned")
            .as_mut()
        {
            adapter.process_event(window, event);
        }
    }
}

impl core::fmt::Debug for A11y {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("A11y")
            .field("attached", &self.is_attached())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::A11y;

    #[test]
    fn a_window_with_no_adapter_publishes_nothing_and_builds_nothing() {
        // The walk of the document is what publishing costs, so a window nothing is listening to
        // must not perform it — and a window with no adapter at all certainly must not.
        let a11y = A11y::default();
        assert!(!a11y.is_attached());

        let mut built = false;
        a11y.publish(&mut || {
            built = true;
            unreachable!("the tree must not be walked when nothing is listening")
        });
        assert!(!built, "the tree was walked with nothing listening");
    }
}
