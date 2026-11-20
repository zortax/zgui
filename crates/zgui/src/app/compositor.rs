//! The compositor a window is drawn through, and its renderer.

use std::sync::Arc;

use compositor_api::{Compositor, FontRegistry};
use compositor_core::CompositorImpl;
use compositor_text::SwashRaster;
use compositor_types::VectorBackend;
use compositor_vello::VelloOptions;
use compositor_wgpu::{Gpu, PrePresent, PresentPacing, SharedGraphics, VectorFactory};
use zgui_platform::{PlatformError, Surface};
use zgui_render::{RenderTarget, Renderer};
use zgui_runtime::compositor::CompositorRenderer;
use zgui_runtime::{AppError, RendererFactory};

/// Reports whether the environment asks for zgui's own renderer, with `ZGUI_PIPELINE=legacy`.
///
/// Read once: the environment does not change under a running process.
pub(crate) fn legacy_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(
        || matches!(std::env::var("ZGUI_PIPELINE"), Ok(value) if value.trim() == "legacy"),
    )
}

/// Returns the factory of renderers that draw each window through its own `compositor-core`
/// compositor and a `compositor-wgpu` renderer on one shared device.
///
/// Every compositor shares `fonts` with layout and rasterises glyphs with swash.
///
/// # Errors
///
/// The factory fails as [`AppError::GpuUnavailable`] when no adapter presents to the window, and
/// as [`AppError::Platform`] when the window offers no handles a graphics API can draw into.
pub(crate) fn factory(fonts: Arc<dyn FontRegistry>) -> RendererFactory {
    let graphics = SharedGraphics::new();
    let raster = Arc::new(SwashRaster::new());
    // The compiled path pipelines are kept where zgui keeps its other pipelines.
    let vello = VelloOptions {
        adapter: None,
        cache_directory: zgui_render_wgpu::gpu::pipeline_cache::directory(),
    };
    Box::new(move |surface, target| {
        let renderer = renderer(&graphics, surface, target, &vello)?;
        let mut compositor = CompositorImpl::new();
        compositor.use_font_registry(Arc::clone(&fonts));
        compositor.use_glyph_raster(Arc::clone(&raster) as _);
        let compositor: Box<dyn Compositor> = Box::new(compositor);
        Ok(Box::new(CompositorRenderer::new(compositor, renderer)) as Box<dyn Renderer>)
    })
}

/// Returns the factory of renderers that draw each window through its own `compositor-core`
/// compositor, sharing `fonts` and rasterising glyphs with swash, onto the renderer `make` returns.
pub(crate) fn wrapping(
    fonts: Arc<dyn FontRegistry>,
    mut make: crate::app::CompositorRendererMaker,
) -> RendererFactory {
    let raster = Arc::new(SwashRaster::new());
    Box::new(move |_surface, target| {
        let size = [
            u32::try_from(target.size.width).unwrap_or(1),
            u32::try_from(target.size.height).unwrap_or(1),
        ];
        let mut compositor = CompositorImpl::new();
        compositor.use_font_registry(Arc::clone(&fonts));
        compositor.use_glyph_raster(Arc::clone(&raster) as _);
        let mut window = CompositorRenderer::new(Box::new(compositor), make(size));
        window.configure(target);
        Ok(Box::new(window) as Box<dyn Renderer>)
    })
}

/// Opens a renderer presenting into `surface` on the shared device, whose path rasteriser is built
/// with `vello`.
fn renderer(
    graphics: &SharedGraphics,
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
    vello: &VelloOptions,
) -> Result<Box<dyn compositor_api::Renderer>, AppError> {
    let Some(handles) = Arc::clone(surface).gpu_shared() else {
        return Err(AppError::Platform(PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        )));
    };
    let pacing = match surface.present_pacing() {
        zgui_platform::PresentPacing::Platform => PresentPacing::Platform,
        _ => PresentPacing::Display,
    };
    let size = [
        u32::try_from(target.size.width).unwrap_or(1),
        u32::try_from(target.size.height).unwrap_or(1),
    ];
    let mut renderer = graphics
        .renderer_for_surface(
            size,
            target.opaque,
            handles,
            pacing,
            notify_wanted(pacing).then(|| pre_present(surface)),
        )
        .map_err(|unavailable| AppError::GpuUnavailable(unavailable_of(unavailable)))?;
    // Without a path rasteriser, paths and glyphs on the path route draw nothing.
    renderer.set_vector_factory(vectors(vello.clone()), VectorBackend::Compute, "vello");
    // A platform that paces its own frames also decides when a composed frame is handed over.
    if pacing == PresentPacing::Platform {
        let gate = Arc::clone(surface);
        renderer.set_present_gate(Box::new(move || gate.present_now()));
    }
    Ok(Box::new(renderer))
}

/// Reports whether `surface` is told that a frame is about to be presented, as
/// `graphics::notify_wanted` decides for zgui's own renderer.
fn notify_wanted(pacing: PresentPacing) -> bool {
    pacing == PresentPacing::Platform || crate::app::graphics::pacing_override()
}

/// Returns the notification that tells `surface` a frame is about to be presented.
fn pre_present(surface: &Arc<dyn Surface>) -> PrePresent {
    let surface = Arc::clone(surface);
    Box::new(move |target: compositor_types::RenderTarget| {
        let size = zgui_geom::Size::new(
            i32::try_from(target.size[0]).unwrap_or(i32::MAX),
            i32::try_from(target.size[1]).unwrap_or(i32::MAX),
        );
        surface.pre_present_frame(size, f64::from(target.scale));
    })
}

/// Returns the factory of the path rasteriser a device can run, built with `options`.
fn vectors(options: VelloOptions) -> VectorFactory {
    Box::new(move |gpu: &Arc<Gpu>, size| {
        compositor_vello::for_device(gpu.adapter(), gpu.device(), gpu.queue(), size, &options)
    })
}

/// Returns the adapters the compositor's renderer tried, in zgui's vocabulary.
fn unavailable_of(unavailable: compositor_types::GpuUnavailable) -> zgui_render::GpuUnavailable {
    let mut out = zgui_render::GpuUnavailable::default();
    for candidate in unavailable.candidates {
        out = out.rejected(candidate.name, candidate.reason);
    }
    out
}
