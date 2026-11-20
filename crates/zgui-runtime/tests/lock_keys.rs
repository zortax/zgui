//! The lock keys a window is told about, and the modifiers its input is stamped with.
//!
//! A change of the modifier state carries caps lock and num lock. The window handle reports the
//! locks, and the held set an unstamped event takes stays free of them.

mod support;

use zgui_platform::{Surface, SurfaceEvent};
use zgui_reactive::prelude::GetUntracked;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::Modifiers;

#[test]
fn a_change_of_the_lock_keys_reaches_the_window_handle() {
    let mut harness = support::app("", |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::column().into_view().build(cx))
    });
    harness.settle(8);
    let handle = harness.app().windows()[0].handle().clone();
    let surface = harness.platform().offscreens()[0].id();
    assert!(!handle.caps_lock().get_untracked());
    assert_eq!(handle.lock_keys().get_untracked(), Modifiers::NONE);

    harness.deliver(
        surface,
        SurfaceEvent::ModifiersChanged(Modifiers::SHIFT | Modifiers::CAPS_LOCK),
    );
    harness.settle(4);
    assert!(handle.caps_lock().get_untracked());
    assert_eq!(
        handle.lock_keys().get_untracked(),
        Modifiers::CAPS_LOCK,
        "the held modifiers are not lock keys"
    );

    harness.deliver(surface, SurfaceEvent::ModifiersChanged(Modifiers::NUM_LOCK));
    harness.settle(4);
    assert!(!handle.caps_lock().get_untracked());
    assert_eq!(handle.lock_keys().get_untracked(), Modifiers::NUM_LOCK);
}
