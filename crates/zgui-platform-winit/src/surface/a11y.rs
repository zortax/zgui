//! The accessibility channels the windows of this loop talk to.
//!
//! # Why the tree is built by a closure and not passed in
//!
//! Building an accessibility tree costs a walk of the whole document. On a machine with no
//! assistive technology running — which is most machines, most of the time — that walk would be
//! pure waste on every frame, so the adapter is asked whether anything is listening and the walk
//! only happens if something is.
//!
//! # Why the adapters live here and not in the surface
//!
//! A surface is shared and usable from any thread, because a frame can be asked for from any
//! thread. An adapter is the opposite: on Windows it owns a window handle and on macOS a graph of
//! Objective-C objects, neither of which may leave the thread that made them. The two cannot live
//! in one object, so the adapters stay on the thread that owns them and a surface names its own by
//! number.
//!
//! Everything that touches an adapter already runs there — they are created while a window is
//! created, fed from the loop's own event callback, and read from the frame's last phase. A call
//! from any other thread finds nothing, publishes nothing and builds nothing, which is the same
//! answer a machine with no assistive technology gives.

use std::cell::RefCell;
use std::collections::HashMap;

use accesskit::TreeUpdate;
use accesskit_winit::Adapter;
use winit::event::WindowEvent;
use winit::window::Window;
use zgui_platform::SurfaceId;

thread_local! {
    /// The adapters belonging to the loop running on this thread.
    ///
    /// A process may create exactly one event loop, so this is that loop's set. Tests that build
    /// adapters on threads of their own get one set each, and they do not see each other's.
    static ADAPTERS: RefCell<HashMap<SurfaceId, Adapter>> = RefCell::new(HashMap::new());
}

/// Holds `adapter` for the window `id` names.
pub(crate) fn attach(id: SurfaceId, adapter: Adapter) {
    ADAPTERS.with_borrow_mut(|adapters| {
        adapters.insert(id, adapter);
    });
}

/// Lets go of the adapter for `id`, on the thread that made it.
///
/// Called where the window itself is destroyed, so that the adapter and the window it speaks for go
/// at the same moment and on the loop's own thread. An adapter released anywhere else would be
/// releasing a window handle, or an Objective-C object, from a thread that has no business
/// touching it.
pub(crate) fn release(id: SurfaceId) {
    let _ = ADAPTERS.try_with(|adapters| adapters.borrow_mut().remove(&id));
}

/// Whether an adapter was ever attached for `id` and is reachable from here.
pub(crate) fn is_attached(id: SurfaceId) -> bool {
    ADAPTERS
        .try_with(|adapters| adapters.borrow().contains_key(&id))
        .unwrap_or(false)
}

/// Publishes an update for `id`, building it only if something is listening.
pub(crate) fn publish(id: SurfaceId, build: &mut dyn FnMut() -> TreeUpdate) {
    with(id, |adapter| adapter.update_if_active(build));
}

/// Lets the adapter for `id` see a window event before anything else does.
///
/// The adapter tracks where the window is and whether it has focus, and it learns both only from
/// the events it is shown. One not shown to it reports the window's bounds from before the move,
/// which is what puts a screen reader's highlight in the wrong place.
pub(crate) fn observe(id: SurfaceId, window: &Window, event: &WindowEvent) {
    with(id, |adapter| adapter.process_event(window, event));
}

/// Runs `act` against the adapter for `id`, if this thread holds one.
///
/// The set stays borrowed for the length of `act`, which for a publish is the walk of the document
/// the update is built from. A walk that reached back in here would find the borrow open and panic:
/// louder than the deadlock the same re-entry used to produce, and reachable from neither caller.
fn with(id: SurfaceId, act: impl FnOnce(&mut Adapter)) {
    let _ = ADAPTERS.try_with(|adapters| {
        if let Some(adapter) = adapters.borrow_mut().get_mut(&id) {
            act(adapter);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{is_attached, publish};
    use zgui_platform::SurfaceId;

    #[test]
    fn a_window_with_no_adapter_publishes_nothing_and_builds_nothing() {
        // The walk of the document is what publishing costs, so a window nothing is listening to
        // must not perform it — and a window with no adapter at all certainly must not.
        let id = SurfaceId::new(1);
        assert!(!is_attached(id));

        let mut built = false;
        publish(id, &mut || {
            built = true;
            unreachable!("the tree must not be walked when nothing is listening")
        });
        assert!(!built, "the tree was walked with nothing listening");
    }

    #[test]
    fn another_thread_reaches_no_adapter_and_builds_nothing() {
        // A frame can be asked for from any thread, so `publish` can be reached from one. What it
        // must never do there is touch an adapter, and the set it finds is empty for that reason.
        let id = SurfaceId::new(2);
        std::thread::spawn(move || {
            assert!(!is_attached(id));
            let mut built = false;
            publish(id, &mut || {
                built = true;
                unreachable!("the tree must not be walked off the loop's thread")
            });
            assert!(!built, "the tree was walked on a thread with no adapter");
        })
        .join()
        .expect("the thread ran");
    }
}
