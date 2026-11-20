//! The presentation timing a window handle reads from its surface.

mod support;

use zgui_platform_headless::{Harness, Headless};
use zgui_runtime::{App, Runtime};
use zgui_view::{BuildCx, IntoView, View};

/// An application with one window.
fn window() -> Harness<Runtime> {
    let handler = App::new()
        .with_renderer(Box::new(support::capture))
        .into_handler(|cx: &mut BuildCx<'_>| {
            Box::new(zgui_elements::column().into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::over(Headless::new(), handler);
    harness.settle(8);
    harness
}

#[test]
fn a_window_on_a_desktop_without_timing_reports_none() {
    let harness = window();
    let handle = harness.app().windows()[0].handle().clone();
    assert!(handle.is_open());
    assert_eq!(handle.presentation_timing(), None);
}

#[test]
fn a_closed_window_reports_no_timing() {
    let mut harness = window();
    let handle = harness.app().windows()[0].handle().clone();
    handle.close();
    harness.settle(8);
    assert_eq!(handle.presentation_timing(), None);
}
