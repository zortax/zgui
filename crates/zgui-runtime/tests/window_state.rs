//! How a maximise or full-screen change with no resize reaches the window handle.
//!
//! A desktop can change either state and keep the size of the window. The platform then reports
//! the change as a `SurfaceStateChanged` wake, and that wake is the only thing that moves the
//! handle's signals.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_geom::Edges;
use zgui_platform::{
    DecorationSource, Decorations, FullscreenMode, PlatformCx, Surface, WakeReason,
};
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

#[test]
fn tiled_edges_decoration_source_and_window_capabilities_update_reactively() {
    let mut harness = support::app("", |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::column().into_view().build(cx))
    });
    harness.settle(8);

    let handle = harness.app().windows()[0].handle().clone();
    let surface_id = harness.platform().offscreens()[0].id();
    handle.set_decorations(Decorations::None);
    let tiled_runs = counting(handle.tiled());
    let decorated_runs = counting(handle.server_decorated());
    let minimize_runs = counting(handle.can_minimize());
    let maximize_runs = counting(handle.can_maximize());
    harness.settle(8);

    assert_eq!(handle.tiled().get_untracked(), Edges::NONE);
    assert!(!handle.tiled().get_untracked().any());
    assert!(
        !handle.server_decorated().get_untracked(),
        "a window set to Decorations::None is client-decorated until a compositor overrides it"
    );
    assert!(!handle.can_minimize().get_untracked());
    assert!(!handle.can_maximize().get_untracked());

    let tiled_before = tiled_runs.get();
    let decorated_before = decorated_runs.get();
    let minimize_before = minimize_runs.get();
    let maximize_before = maximize_runs.get();

    // Simulate a tiling compositor configuring tiled edges, forcing SSD, and advertising maximize
    // without minimize.
    let grid_edges = Edges::new(true, false, true, true);
    harness.platform().set_surface_tiled(surface_id, grid_edges);
    harness
        .platform()
        .set_surface_decoration_source(surface_id, Some(DecorationSource::Platform));
    harness
        .platform()
        .set_surface_window_capabilities(surface_id, Some(false), Some(true));
    harness.settle(8);

    assert_eq!(handle.tiled().get_untracked(), grid_edges);
    assert!(handle.tiled().get_untracked().any());
    assert!(!handle.tiled().get_untracked().all());
    assert!(handle.server_decorated().get_untracked());
    assert!(!handle.can_minimize().get_untracked());
    assert!(handle.can_maximize().get_untracked());
    assert_eq!(tiled_runs.get() - tiled_before, 1);
    assert_eq!(decorated_runs.get() - decorated_before, 1);
    assert_eq!(
        minimize_runs.get() - minimize_before,
        0,
        "an unchanged capability wakes no subscriber"
    );
    assert_eq!(maximize_runs.get() - maximize_before, 1);

    harness.shut_down();
}
