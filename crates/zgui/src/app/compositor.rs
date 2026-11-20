//! The compositor every window is drawn through, and the renderers that draw its frames.

use std::sync::Arc;

use compositor_api::{Compositor, Renderer};
use compositor_core::CompositorImpl;
use compositor_text::SwashRaster;
use compositor_types::{RenderTarget, VectorBackend};
use compositor_vello::VelloOptions;
use compositor_wgpu::{
    Gpu, LatencyMarks, PrePresent, PresentPacing, SharedGraphics, VectorFactory,
};
use zgui_platform::{PlatformError, Surface};
use zgui_runtime::{AppError, RendererFactory};

/// Returns the compositor an application's windows draw through, drawing text in `fonts`.
///
/// It is the reference compositor, and it rasterises glyphs with swash.
pub fn compositor(fonts: &crate::app::Fonts) -> Box<dyn Compositor> {
    let mut compositor = CompositorImpl::new();
    compositor.use_font_registry(fonts.compositor_registry());
    compositor.use_glyph_raster(Arc::new(SwashRaster::new()));
    Box::new(compositor)
}

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
    let graphics = SharedGraphics::new().with_options(pipeline_directory(), Some(latency_marks()));
    // The compiled path pipelines are kept beside the renderer's own.
    let vello = VelloOptions {
        adapter: None,
        cache_directory: pipeline_directory(),
    };
    let precision = crate::GroupPrecision::from_environment()
        .or(precision)
        .unwrap_or_default()
        .wgpu();
    Box::new(move |surface, target| renderer(&graphics, surface, target, &vello, precision))
}

/// The variable that names the directory compiled pipelines are kept in.
pub const PIPELINE_DIRECTORY_VARIABLE: &str = "ZGUI_CACHE_DIR";

/// Returns the directory compiled pipelines are kept in between runs: the one
/// [`PIPELINE_DIRECTORY_VARIABLE`] names, then `zgui/pipelines` under `XDG_CACHE_HOME` or under
/// `~/.cache`.
///
/// Returns `None` when the environment names none of them.
pub fn pipeline_directory() -> Option<std::path::PathBuf> {
    if let Ok(named) = std::env::var(PIPELINE_DIRECTORY_VARIABLE) {
        return Some(named.into());
    }
    let home = std::env::var("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        })
        .ok()?;
    Some(home.join("zgui").join("pipelines"))
}

/// Returns the marks a renderer emits at the steps of a frame, recorded into zgui's latency traces.
pub fn latency_marks() -> LatencyMarks {
    Arc::new(|stage, detail| match detail {
        Some(detail) => zgui_profile::latency::note(stage, detail),
        None => zgui_profile::latency::mark(stage),
    })
}

/// Returns the factory of the renderers `make` returns, configured for each window's target.
pub(crate) fn wrapping(mut make: crate::app::CompositorRendererMaker) -> RendererFactory {
    Box::new(move |_surface, target| {
        let mut renderer = make(target.size);
        renderer.configure(target);
        Ok(renderer)
    })
}

/// Opens a renderer presenting into `surface` on the shared device, whose path rasteriser is built
/// with `vello`.
fn renderer(
    graphics: &SharedGraphics,
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
    vello: &VelloOptions,
    precision: compositor_wgpu::GroupPrecision,
) -> Result<Box<dyn Renderer>, AppError> {
    let Some(handles) = Arc::clone(surface).gpu_shared() else {
        return Err(AppError::Platform(PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        )));
    };
    let pacing = match surface.present_pacing() {
        zgui_platform::PresentPacing::Platform => PresentPacing::Platform,
        _ => PresentPacing::Display,
    };
    let mut renderer = graphics
        .renderer_for_surface(
            target.size,
            target.opaque,
            handles,
            pacing,
            notify_wanted(pacing).then(|| pre_present(surface)),
        )
        .map_err(AppError::GpuUnavailable)?;
    renderer.set_group_precision(precision);
    // Without a path rasteriser, paths and glyphs on the path route draw nothing.
    renderer.set_vector_factory(vectors(vello.clone()), VectorBackend::Compute, "vello");
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

/// Returns the factory of the path rasteriser a device can run, built with `options`.
pub(crate) fn vectors(options: VelloOptions) -> VectorFactory {
    Box::new(move |gpu: &Arc<Gpu>, size| {
        compositor_vello::for_device(gpu.adapter(), gpu.device(), gpu.queue(), size, &options)
    })
}
