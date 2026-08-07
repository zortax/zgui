//! What a window is drawn through.

use std::sync::Arc;

use zgui_platform::{PlatformError, Surface};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{SharedGraphics, renderer::PrePresent};
use zgui_runtime::{AppError, RendererFactory};

/// The compositor notification belonging to `surface`, kept alive with it.
fn pre_present(surface: &Arc<dyn Surface>) -> PrePresent {
    let surface = Arc::clone(surface);
    pre_present_callback(move || surface.pre_present_notify())
}

/// Boxes a platform notification for the renderer seam.
fn pre_present_callback(notify: impl Fn() + Send + Sync + 'static) -> PrePresent {
    Box::new(notify)
}

/// The renderer factory an application's windows are drawn through.
///
/// One graphics device behind all of them: the factory is called once per window and keeps the
/// device, its pipelines and its compiled-shader cache between calls. A device per window would
/// cost another driver connection and another copy of every fixed buffer the vector rasteriser
/// allocates, none of which is per-window work. The device itself is opened by the first window,
/// because which adapter is usable is only known against something to present to.
///
/// # Errors
///
/// The factory returns [`AppError::GpuUnavailable`] when no adapter on this machine could present
/// to the window, naming every one that was tried, and [`AppError::Platform`] when the window
/// offers no handles a graphics API can draw into at all. Neither is answered with a renderer that
/// draws nowhere: a window that opens and never paints looks like a program that has hung.
pub(crate) fn factory() -> RendererFactory {
    let graphics = SharedGraphics::new();
    Box::new(move |surface, target| renderer(&graphics, surface, target))
}

/// Opens a renderer for `surface` on the shared graphics.
fn renderer(
    graphics: &SharedGraphics,
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let Some(handles) = Arc::clone(surface).gpu_shared() else {
        return Err(AppError::Platform(PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        )));
    };

    // The surface has to be created from the instance the device is opened from, which is why the
    // window's handles go to the renderer rather than the other way round. The shared handle keeps
    // the window alive for as long as anything draws through it.
    let drawable = graphics
        .instance()
        .create_surface(handles)
        .map_err(|error| PlatformError::Backend(error.to_string()))?;
    let mut renderer =
        graphics.renderer_for_surface(target, drawable, Some(pre_present(surface)))?;
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

    #[test]
    fn the_factory_is_built_without_touching_a_graphics_device() {
        // Making the factory must not enumerate adapters: it is built while the application is
        // still being described, on a thread that may have no display connection yet, and the
        // device is opened by the first window instead.
        let _factory = factory();
    }
}
