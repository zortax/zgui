//! A shell window, as the application holds it.
//!
//! What a window handle asks of a shell layer has to reach the surface, and what it asks of an
//! ordinary window has to do nothing at all.

mod support;

use zgui_geom::{CssPx, Edges};
use zgui_platform::{ExclusiveZone, KeyboardInteractivity, LayerPatch, Surface, SurfaceRole};
use zgui_platform_headless::Harness;
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
