//! What a window is drawn through.

use std::sync::Arc;

use zgui_platform::{PlatformError, Surface};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{PresentPacing, SharedGraphics, renderer::PrePresent};
use zgui_runtime::{AppError, RendererFactory};

/// The compositor notification belonging to `surface`, kept alive with it.
fn pre_present(surface: &Arc<dyn Surface>) -> PrePresent {
    let surface = Arc::clone(surface);
    pre_present_callback(move || surface.pre_present_notify())
}

/// Whether `surface` is told that a frame is about to be presented.
///
/// A backend that paces frames itself is always told, because the notification is how it learns
/// that a buffer is being committed and is where it asks the compositor for the next frame. On a
/// backend that leaves the wait to the display it is unwired by default, and
/// `ZGUI_PRESENT_PACING=notify` is what asks for it.
///
/// The default is off because of what the notification does under winit on Wayland: it requests a
/// frame callback per present and then withholds every redraw until the callback arrives, which
/// serialises the present against the next frame's processor work and quantises the frame period
/// to whole refresh intervals — a window whose frame costs a little over one interval presents at
/// half its output's rate. See `docs/research/frame-callback-quantization.md`. The starvation
/// stall the notification used to suppress is answered by the runtime's starvation latch instead.
fn notify_wanted(pacing: PresentPacing) -> bool {
    pacing == PresentPacing::Platform || pacing_override()
}

/// Whether the environment asked for the notification on a display-paced backend.
///
/// Read once: the environment does not change under a running process.
fn pacing_override() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(
        || matches!(std::env::var("ZGUI_PRESENT_PACING"), Ok(value) if value.trim() == "notify"),
    )
}

/// What `pacing` this surface's platform asks the renderer to configure for.
fn pacing_of(surface: &Arc<dyn Surface>) -> PresentPacing {
    match surface.present_pacing() {
        zgui_platform::PresentPacing::Platform => PresentPacing::Platform,
        _ => PresentPacing::Display,
    }
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
    let pacing = pacing_of(surface);
    let mut renderer = graphics.renderer_for_surface(
        target,
        drawable,
        pacing,
        notify_wanted(pacing).then(|| pre_present(surface)),
    )?;
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
