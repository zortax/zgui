//! A window whose renderer cannot be built.

mod support;

use std::sync::Arc;

use compositor_types::{GpuUnavailable, RenderTarget};
use zgui_platform::{PlatformCx, Surface};
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError};
use zgui_view::{BuildCx, IntoView, View};

/// A renderer factory whose renderer the driver fails to build.
fn failing(
    _surface: &Arc<dyn Surface>,
    _target: RenderTarget,
) -> Result<compositor_api::RendererFactory, AppError> {
    Ok(Box::new(|| Err(GpuUnavailable::default())))
}

#[test]
fn a_launch_window_whose_renderer_fails_stops_the_application_with_the_error() {
    let handler = App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_compositor(support::client())
        .with_renderer(Box::new(failing))
        .into_handler(|cx: &mut BuildCx<'_>| {
            Box::new(zgui_elements::column().into_view().build(cx)) as Box<dyn zgui_view::Anchor>
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(8);

    assert!(
        harness.platform().is_exiting(),
        "the application kept running"
    );
    assert!(
        matches!(
            harness.app().failure().take(),
            Some(AppError::GpuUnavailable(_))
        ),
        "the failure was not reported"
    );
}
