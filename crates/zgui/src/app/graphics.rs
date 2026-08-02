//! What a window is drawn through.

use std::sync::Arc;

use zgui_platform::{PlatformError, Surface};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::Builder;
use zgui_runtime::AppError;

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

    let builder = Builder::new();
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
