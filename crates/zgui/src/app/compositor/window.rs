//! The renderers that present into windows, built on the thread that draws their frames.

use std::sync::Arc;

use compositor_api::Renderer;
use compositor_types::{RenderTarget, VectorBackend};
use compositor_vello::VelloOptions;
use compositor_wgpu::{LatencyMarks, PrePresent, PresentPacing, SharedGraphics, SurfacePresenter};
use zgui_platform::{GpuSurface, PlatformError, Surface};
use zgui_runtime::{AppError, RendererFactory};

use crate::app::compositor::{PathRenderers, latency_marks, pipeline_directory, vectors};

/// Returns the factory of `compositor-wgpu` renderers that present into each window, all on one
/// shared device.
///
/// Each renderer holds its groups at `precision`, which `ZGUI_GROUP_PRECISION` overrides.
///
/// # Errors
///
/// The factory fails as [`AppError::GpuUnavailable`] when no adapter presents to the window, and
/// as [`AppError::Platform`] when the window offers no handles a graphics API can draw into.
pub(crate) fn factory(precision: Option<crate::GroupPrecision>) -> RendererFactory {
    let options = GraphicsOptions {
        pipelines: pipeline_directory(),
        marks: latency_marks(),
    };
    // The compiled path pipelines are kept beside the renderer's own.
    let vello = VelloOptions {
        adapter: None,
        cache_directory: pipeline_directory(),
    };
    let precision = crate::GroupPrecision::from_environment()
        .or(precision)
        .unwrap_or_default()
        .wgpu();
    let paths = PathRenderers::default();
    Box::new(move |surface, target| {
        let handles = handles(surface)?;
        let surface = Arc::clone(surface);
        let (vello, paths, options) = (vello.clone(), paths.clone(), options.clone());
        Ok(Box::new(move || {
            let graphics = graphics(&options);
            renderer(
                &graphics,
                &surface,
                handles,
                target,
                (&vello, &paths),
                precision,
            )
        }))
    })
}

/// The options the shared device of a drawing thread is opened with.
#[derive(Clone)]
struct GraphicsOptions {
    pipelines: Option<std::path::PathBuf>,
    marks: LatencyMarks,
}

thread_local! {
    /// The device every renderer built on this thread draws on.
    // Leaked, because the renderers it opened can outlive the thread's other locals, and dropping
    // the device after them panics at thread exit.
    static GRAPHICS: std::cell::OnceCell<&'static SharedGraphics> =
        const { std::cell::OnceCell::new() };
}

/// Returns the device every renderer built on this thread shares, opened with `options` by the
/// first renderer.
fn graphics(options: &GraphicsOptions) -> SharedGraphics {
    GRAPHICS.with(|graphics| {
        let shared: &SharedGraphics = graphics.get_or_init(|| {
            Box::leak(Box::new(SharedGraphics::new().with_options(
                options.pipelines.clone(),
                Some(Arc::clone(&options.marks)),
            )))
        });
        shared.clone()
    })
}

/// Returns the handles of `surface` a graphics API draws into.
///
/// # Errors
///
/// Fails as [`AppError::Platform`] when the window offers none.
fn handles(surface: &Arc<dyn Surface>) -> Result<Arc<dyn GpuSurface>, AppError> {
    Arc::clone(surface).gpu_shared().ok_or_else(|| {
        AppError::Platform(PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        ))
    })
}

/// Opens a renderer presenting into `surface` through `handles` on the shared device, whose path
/// rasteriser is built with `vello` and shares the device's path renderer in `paths`.
fn renderer(
    graphics: &SharedGraphics,
    surface: &Arc<dyn Surface>,
    handles: Arc<dyn GpuSurface>,
    target: RenderTarget,
    (vello, paths): (&VelloOptions, &PathRenderers),
    precision: compositor_wgpu::GroupPrecision,
) -> Result<Box<dyn Renderer>, compositor_types::GpuUnavailable> {
    let pacing = match surface.present_pacing() {
        zgui_platform::PresentPacing::Platform => PresentPacing::Platform,
        _ => PresentPacing::Display,
    };
    let notified = notify_wanted(pacing);
    let mut renderer = graphics.renderer_for_surface(
        target.size,
        target.opaque,
        handles,
        pacing,
        notified.then(|| pre_present(surface)),
    )?;
    renderer.set_group_precision(precision);
    // The notification sets the extent the surface shows, so a cropping surface keeps its swap
    // chain through a resize within a size class and skips the wait for an idle device.
    if notified
        && surface.crops_buffers()
        && let Some(presenter) = renderer
            .wgpu_presenter()
            .as_any_mut()
            .and_then(|presenter| presenter.downcast_mut::<SurfacePresenter>())
    {
        presenter.set_crop(true);
    }
    // Without a path rasteriser, paths and glyphs on the path route draw nothing.
    renderer.set_vector_factory(
        vectors(vello.clone(), paths.clone()),
        VectorBackend::Compute,
        "vello",
    );
    // A platform that paces its own frames also decides when a composed frame is handed over.
    if pacing == PresentPacing::Platform {
        let gate = Arc::clone(surface);
        renderer.set_present_gate(Box::new(move || gate.present_now()));
    }
    Ok(Box::new(renderer))
}

/// Reports whether a surface paced by `pacing` is told that a frame is about to be presented.
///
/// A backend that paces frames itself is always told, because the notification is how it learns
/// that a buffer is being committed and is where it asks for the next frame. On a backend that
/// leaves the wait to the display it is unwired by default, and `ZGUI_PRESENT_PACING=notify` asks
/// for it.
fn notify_wanted(pacing: PresentPacing) -> bool {
    pacing == PresentPacing::Platform || pacing_override()
}

/// Reports whether the environment asked for the notification on a display-paced backend.
///
/// Read once: the environment does not change under a running process.
fn pacing_override() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(
        || matches!(std::env::var("ZGUI_PRESENT_PACING"), Ok(value) if value.trim() == "notify"),
    )
}

/// Returns the notification that tells `surface` a frame is about to be presented.
fn pre_present(surface: &Arc<dyn Surface>) -> PrePresent {
    let surface = Arc::clone(surface);
    Box::new(move |target: RenderTarget| {
        let size = zgui_geom::Size::new(
            i32::try_from(target.size[0]).unwrap_or(i32::MAX),
            i32::try_from(target.size[1]).unwrap_or(i32::MAX),
        );
        surface.pre_present_frame(size, f64::from(target.scale));
    })
}
