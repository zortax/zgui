//! A shell window, as the application holds it.
//!
//! What a window handle asks of a shell layer has to reach the surface, and what it asks of an
//! ordinary window has to do nothing at all. A shell window holds the keyboard only once the
//! desktop says so, and the application hears every edge.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{CssPx, Edges};
use zgui_platform::{
    ExclusiveZone, KeyboardInteractivity, LayerPatch, Surface, SurfaceEvent, SurfaceRole,
};
use zgui_platform_headless::Harness;
use zgui_reactive::prelude::GetUntracked;
use zgui_runtime::windows::{LayerOptions, WindowHandle, WindowOptions};
use zgui_runtime::{App, Runtime};
use zgui_view::{BuildCx, IntoView, View};

/// An application whose primary window is a bar, and a second, ordinary window beside it.
fn bar_and_window() -> (Harness<Runtime>, WindowHandle, WindowHandle) {
    let handler = App::new()
        .with_layer(
            LayerOptions::top()
                .with_anchors(Edges::new(true, true, false, true))
                .with_size(0.0, 34.0)
                .with_exclusive_zone(ExclusiveZone::Reserve(CssPx(34.0))),
        )
        .with_renderer(Box::new(support::capture))
        .into_handler(|cx: &mut BuildCx<'_>| {
            Box::new(zgui_elements::column().into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(8);
    let bar = harness.app().windows()[0].handle().clone();
    let window = harness.app().scope().with(|| {
        zgui_runtime::use_windows().open(WindowOptions::new("ordinary"), zgui_elements::column)
    });
    harness.settle(8);
    (harness, bar, window)
}

#[test]
fn a_placement_change_reaches_the_shell_window_and_changes_only_what_it_names() {
    let (mut harness, bar, _window) = bar_and_window();
    let surface = harness.platform().offscreens()[0].clone();

    let patch = LayerPatch::new()
        .with_exclusive_zone(ExclusiveZone::Reserve(CssPx(50.0)))
        .with_keyboard(KeyboardInteractivity::OnDemand);
    bar.set_layer_placement(patch.clone());
    harness.settle(8);

    assert_eq!(surface.layer_log(), [patch]);
    let SurfaceRole::Layer(placement) = surface.requested_attributes().role else {
        panic!("the bar is a shell window");
    };
    assert_eq!(
        placement.exclusive_zone,
        ExclusiveZone::Reserve(CssPx(50.0))
    );
    assert_eq!(placement.keyboard, KeyboardInteractivity::OnDemand);
    assert_eq!(
        placement.anchors,
        Edges::new(true, true, false, true),
        "the anchors were not named, so they stay"
    );
    harness.shut_down();
}

#[test]
fn a_placement_change_on_an_ordinary_window_does_nothing() {
    let (mut harness, _bar, window) = bar_and_window();
    let ordinary = harness
        .platform()
        .offscreens()
        .into_iter()
        .find(|surface| matches!(surface.requested_attributes().role, SurfaceRole::Toplevel))
        .expect("the ordinary window opened");
    window.set_layer_placement(LayerPatch::new().with_exclusive_zone(ExclusiveZone::Ignore));
    harness.settle(8);
    assert!(ordinary.layer_log().is_empty());
    assert!(
        ordinary.set_layer_placement(&LayerPatch::new()).is_err(),
        "an ordinary window refuses a shell placement"
    );
    harness.shut_down();
}

#[test]
fn a_shell_window_starts_without_the_keyboard_and_an_ordinary_one_with_it() {
    let (mut harness, bar, window) = bar_and_window();
    assert!(
        !bar.focused().get_untracked(),
        "a shell window is given the keyboard by an edge of its own"
    );
    assert!(window.focused().get_untracked());
    harness.shut_down();
}

#[test]
fn every_focus_edge_reaches_the_application_even_two_in_one_frame() {
    let (mut harness, bar, _window) = bar_and_window();
    let surface = harness.platform().offscreens()[0].id();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let guard = {
        let heard = Rc::clone(&heard);
        bar.on_focus_change(move |focused| heard.borrow_mut().push(focused))
    };

    harness.deliver(surface, SurfaceEvent::Focused(true));
    harness.settle(8);
    assert_eq!(*heard.borrow(), [true]);
    assert!(bar.focused().get_untracked());

    // A gain and a loss before the next frame: the signal ends where it started, and the
    // application still hears both.
    harness.deliver(surface, SurfaceEvent::Focused(false));
    harness.deliver(surface, SurfaceEvent::Focused(true));
    harness.deliver(surface, SurfaceEvent::Focused(false));
    harness.settle(8);
    assert_eq!(*heard.borrow(), [true, false, true, false]);
    assert!(!bar.focused().get_untracked());

    // A report of the state the window is already in is not an edge.
    harness.deliver(surface, SurfaceEvent::Focused(false));
    harness.settle(8);
    assert_eq!(heard.borrow().len(), 4);

    drop(guard);
    harness.deliver(surface, SurfaceEvent::Focused(true));
    harness.settle(8);
    assert_eq!(heard.borrow().len(), 4, "the guard took the callback away");
    harness.shut_down();
}

#[test]
fn a_window_opened_hidden_stays_hidden_after_its_first_frame_until_it_is_shown() {
    let (mut harness, _bar, _window) = bar_and_window();
    let popover = harness.app().scope().with(|| {
        zgui_runtime::use_windows().open(
            WindowOptions::layer(LayerOptions::top().with_size(320.0, 200.0)).with_visible(false),
            zgui_elements::column,
        )
    });
    harness.settle(8);
    let surface = harness
        .platform()
        .offscreens()
        .last()
        .cloned()
        .expect("the pop-over opened");
    assert!(
        harness.app().windows().len() == 3,
        "the pop-over has a window"
    );
    assert!(!surface.is_visible(), "the first frame did not show it");

    popover.set_visible(true);
    harness.settle(8);
    assert!(surface.is_visible());
    harness.shut_down();
}
