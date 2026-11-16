//! The reduced-motion preference, from the desktop and from the application, as the media
//! feature every window answers.

mod support;

use zgui_platform_headless::{Harness, Headless};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_runtime::{App, Runtime};
use zgui_view::{BuildCx, IntoView, View};

/// A probe ten pixels wide, fifty under `prefers-reduced-motion: reduce`.
const CSS: &str = "root { display: block } .probe { display: block; width: 10px; height: 10px }
                   @media (prefers-reduced-motion: reduce) { .probe { width: 50px } }";

/// The application, with the probe in its window.
fn app() -> App {
    App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_renderer(Box::new(support::capture))
}

/// Builds the probe.
fn probe(cx: &mut BuildCx<'_>) -> Box<dyn zgui_view::Anchor> {
    let view = zgui_elements::r#box().class("probe").into_view();
    Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
}

/// The probe's width in the first window, in device pixels.
fn width(harness: &Harness<Runtime>) -> f32 {
    let window = harness.app().windows().first().expect("a window");
    window
        .box_with_class("probe")
        .expect("the probe is laid out")
        .size
        .width
        .0
}

#[test]
fn a_desktop_with_no_preference_answers_no_preference() {
    let handler = app().into_handler(probe).expect("the runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(8);
    assert_eq!(width(&harness), 10.0);
    assert!(!harness.app().windows()[0].reduced_motion());
}

#[test]
fn the_desktop_preference_reaches_the_first_frame_and_follows_changes() {
    let platform = Headless::new();
    platform.set_reduced_motion(Some(true));
    let handler = app().into_handler(probe).expect("the runtime installs");
    let mut harness = Harness::over(platform, handler);
    harness.settle(8);
    assert_eq!(width(&harness), 50.0, "the first frame reads the desktop");

    harness.platform().set_reduced_motion(Some(false));
    harness.settle(8);
    assert_eq!(width(&harness), 10.0, "the window follows the desktop");

    let desktop = harness
        .app()
        .scope()
        .with(zgui_runtime::use_reduced_motion)
        .desktop();
    assert_eq!(desktop.get_untracked(), Some(false));
}

#[test]
fn the_application_signal_overrides_the_desktop_and_restyles_every_change() {
    let reduce = RwSignal::new(true);
    let platform = Headless::new();
    platform.set_reduced_motion(Some(false));
    let handler = app()
        .with_reduced_motion(reduce.into())
        .into_handler(probe)
        .expect("the runtime installs");
    let mut harness = Harness::over(platform, handler);
    harness.settle(8);
    assert_eq!(width(&harness), 50.0, "the application value wins");

    reduce.set(false);
    harness.settle(8);
    assert_eq!(width(&harness), 10.0);

    reduce.set(true);
    harness.settle(8);
    assert_eq!(width(&harness), 50.0);

    // The desktop moving changes nothing while the application holds a value.
    harness.platform().set_reduced_motion(Some(false));
    harness.settle(8);
    assert_eq!(width(&harness), 50.0);
}

#[test]
fn the_runtime_setter_overrides_and_releases_the_desktop() {
    let platform = Headless::new();
    platform.set_reduced_motion(Some(true));
    let handler = app().into_handler(probe).expect("the runtime installs");
    let mut harness = Harness::over(platform, handler);
    harness.settle(8);
    let motion = harness.app().scope().with(zgui_runtime::use_reduced_motion);

    motion.set(Some(false));
    harness.settle(8);
    assert_eq!(width(&harness), 10.0);
    assert!(!motion.reduced().get_untracked());

    motion.set(None);
    harness.settle(8);
    assert_eq!(width(&harness), 50.0, "`None` follows the desktop again");
}
