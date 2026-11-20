//! How a maximise or full-screen change with no resize reaches the window handle.
//!
//! A desktop can change either state and keep the size of the window. The platform then reports
//! the change as a `SurfaceStateChanged` wake, and that wake is the only thing that moves the
//! handle's signals.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_platform::{FullscreenMode, PlatformCx, Surface, WakeReason};
use zgui_reactive::prelude::{Get, GetUntracked};
use zgui_view::{BuildCx, IntoView, View};

/// An effect counting its own runs, kept for the life of the test.
fn counting<T: Clone + Send + Sync + 'static>(signal: zgui_reactive::Signal<T>) -> Rc<Cell<u32>> {
    let runs = Rc::new(Cell::new(0));
    let seen = Rc::clone(&runs);
    core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
        let _ = signal.get();
        seen.set(seen.get() + 1);
    }));
    runs
}

#[test]
fn a_state_change_without_a_resize_reaches_the_handle_through_the_wake() {
    let mut harness = support::app("", |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::column().into_view().build(cx))
    });
    harness.settle(8);

    let handle = harness.app().windows()[0].handle().clone();
    let surface = harness.platform().offscreens()[0].clone();
    let maximized = counting(handle.maximized());
    let fullscreen = counting(handle.fullscreen());
    harness.settle(8);
    let maximized_before = maximized.get();
    let fullscreen_before = fullscreen.get();

    // The headless desktop grants both at once, with no resize and no event.
    handle.set_maximized(true);
    handle.set_fullscreen(Some(FullscreenMode::Borderless));
    harness.settle(8);
    assert!(
        !handle.maximized().get_untracked(),
        "the handle read the state with nothing reported"
    );

    harness
        .platform()
        .waker()
        .wake(WakeReason::SurfaceStateChanged(surface.id()));
    harness.settle(8);

    assert!(handle.maximized().get_untracked());
    assert_eq!(
        handle.fullscreen().get_untracked(),
        Some(FullscreenMode::Borderless)
    );
    assert_eq!(maximized.get() - maximized_before, 1);
    assert_eq!(fullscreen.get() - fullscreen_before, 1);

    harness.shut_down();
}
