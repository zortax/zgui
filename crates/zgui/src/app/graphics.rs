//! What a window is drawn through.

use std::sync::Arc;

use zgui_platform::{PlatformError, Surface};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, renderer::PrePresent};
use zgui_runtime::AppError;

/// The compositor notification belonging to `surface`, kept alive with it.
fn pre_present(surface: &Arc<dyn Surface>) -> PrePresent {
    let surface = Arc::clone(surface);
    pre_present_callback(move || surface.pre_present_notify())
}

/// Boxes a platform notification for the renderer seam.
fn pre_present_callback(notify: impl Fn() + Send + Sync + 'static) -> PrePresent {
    Box::new(notify)
}

/// Opens a graphics device for `surface` and returns the renderer that draws into it.
///
/// # Errors
///
/// Returns [`AppError::GpuUnavailable`] when no adapter on this machine could present to the
/// window, naming every one that was tried, and [`AppError::Platform`] when the window offers no
/// handles a graphics API can draw into at all. Neither is answered with a renderer that draws
/// nowhere: a window that opens and never paints looks like a program that has hung.
pub(crate) fn renderer(
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let Some(handles) = Arc::clone(surface).gpu_shared() else {
        return Err(AppError::Platform(PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        )));
    };

    let builder = Builder::new().with_pre_present(pre_present(surface));
    // The surface has to be created from the instance the device is opened from, which is why the
    // window's handles go to the renderer rather than the other way round. The shared handle keeps
    // the window alive for as long as anything draws through it.
    let drawable = builder
        .instance()
        .create_surface(handles)
        .map_err(|error| PlatformError::Backend(error.to_string()))?;
    let mut renderer = builder.for_surface(target, drawable)?;
    // Without this a display list's vector passes are planned, counted and then drawn from nothing,
    // so every drawing in the window — every icon — is empty space. Which rasteriser this is comes
    // from what the device turned out to be able to run, and the fallback needs no compute shaders,
    // so there is no machine on which this leaves a window with no drawings in it.
    zgui_render_vector_vello::attach(&mut renderer, target.size);
    Ok(Box::new(renderer))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn the_default_renderer_callback_runs_its_platform_notification() {
        let notifications = Arc::new(AtomicUsize::new(0));
        let recorded = Arc::clone(&notifications);
        let notify = pre_present_callback(move || {
            recorded.fetch_add(1, Ordering::Relaxed);
        });

        notify();

        assert_eq!(
            notifications.load(Ordering::Relaxed),
            1,
            "the renderer callback did not run its platform notification"
        );
    }
}
