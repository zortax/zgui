//! The compositor every window is drawn through, the driver that runs it, and the renderers that
//! draw its frames.

use std::sync::{Arc, Mutex, Weak};

use compositor_api::{Compositor, CompositorBudgets, CompositorClient, Renderer};
use compositor_core::CompositorImpl;
use compositor_text::SwashRaster;
use compositor_types::{RenderTarget, VectorBackend};
use compositor_vello::{Choice, SharedRenderer, VelloOptions, VelloRaster};
use compositor_wgpu::{
    Gpu, LatencyMarks, PrePresent, PresentPacing, SharedGraphics, VectorFactory,
};
use zgui_platform::{GpuSurface, PlatformError, Surface};
use zgui_runtime::budget::limits::ATLAS_BYTES;
use zgui_runtime::{AppError, RendererFactory};

use crate::app::driver::CompositorDriver;

/// Returns the compositor an application's windows draw through, drawing text in `fonts`.
///
/// It is the reference compositor, it rasterises glyphs with swash, and its atlas is held to
/// [`ATLAS_BYTES`].
pub fn compositor(fonts: &crate::app::Fonts) -> Box<dyn Compositor> {
    let mut compositor = CompositorImpl::new();
    compositor.use_font_registry(fonts.compositor_registry());
    compositor.use_glyph_raster(Arc::new(SwashRaster::new()));
    compositor.set_budgets(CompositorBudgets {
        atlas_bytes: Some(ATLAS_BYTES),
    });
    Box::new(compositor)
}

/// Returns `compositor` behind `driver`, which `ZGUI_DRIVER` overrides with `local` or
/// `threaded`.
pub(crate) fn driven(
    compositor: Box<dyn Compositor>,
    driver: CompositorDriver,
) -> Box<dyn CompositorClient> {
    // A page has one thread.
    let driver = if cfg!(target_arch = "wasm32") {
        CompositorDriver::InProcess
    } else {
        CompositorDriver::from_environment().unwrap_or(driver)
    };
    match driver {
        CompositorDriver::Threaded => {
            Box::new(compositor_thread::ThreadedCompositor::spawn(compositor).0)
        }
        CompositorDriver::InProcess => Box::new(compositor_driver::InProcess::new(
            compositor,
            compositor_driver::DriverOptions::default(),
        )),
    }
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
///
/// `make` runs on the thread that draws the window's frames.
pub(crate) fn wrapping(make: crate::app::CompositorRendererMaker) -> RendererFactory {
    let make = Arc::new(Mutex::new(make));
    Box::new(move |_surface, target| {
        let make = Arc::clone(&make);
        Ok(Box::new(move || {
            let mut make = make.lock().unwrap_or_else(|held| held.into_inner());
            let mut renderer = make(target.size);
            renderer.configure(target);
            Ok(renderer)
        }))
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
    let mut renderer = graphics.renderer_for_surface(
        target.size,
        target.opaque,
        handles,
        pacing,
        notify_wanted(pacing).then(|| pre_present(surface)),
    )?;
    renderer.set_group_precision(precision);
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

/// Returns the factory of the path rasteriser a device can run, built with `options`.
///
/// Rasterisers on one device share the vello path renderer that `paths` keeps for it.
pub(crate) fn vectors(options: VelloOptions, paths: PathRenderers) -> VectorFactory {
    Box::new(
        move |gpu: &Arc<Gpu>, size| match paths.shared(gpu, &options) {
            Some(shared) => Box::new(VelloRaster::sharing(shared, gpu.queue())),
            None => compositor_vello::for_device(
                gpu.adapter(),
                gpu.device(),
                gpu.queue(),
                size,
                &options,
            ),
        },
    )
}

/// The vello path renderer of the last device a window drew paths on.
///
/// A second window on the same device shares the renderer and its fixed buffers. The renderer is
/// dropped with the last rasteriser that uses it.
#[derive(Clone, Default)]
pub(crate) struct PathRenderers {
    held: Arc<Mutex<Option<Held>>>,
}

/// A path renderer and the device it was built on, both held weakly.
struct Held {
    device: Weak<Gpu>,
    renderer: Weak<SharedRenderer>,
}

impl PathRenderers {
    /// Returns the path renderer of `gpu`, and builds it with `options` when the device has none.
    ///
    /// Returns `None` when the device cannot run vello or the renderer fails to build.
    fn shared(&self, gpu: &Arc<Gpu>, options: &VelloOptions) -> Option<Arc<SharedRenderer>> {
        if compositor_vello::chosen(gpu.adapter()) != Choice::Compute {
            return None;
        }
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // A replacement device after a loss is another `Gpu`, so it gets a renderer of its own.
        if let Some(entry) = held.as_ref()
            && entry
                .device
                .upgrade()
                .is_some_and(|device| Arc::ptr_eq(&device, gpu))
            && let Some(shared) = entry.renderer.upgrade()
        {
            return Some(shared);
        }
        let options = VelloOptions {
            adapter: options.adapter.clone().or(Some(gpu.adapter().get_info())),
            cache_directory: options.cache_directory.clone(),
        };
        let shared = SharedRenderer::new(gpu.device(), &options).ok()?;
        *held = Some(Held {
            device: Arc::downgrade(gpu),
            renderer: Arc::downgrade(&shared),
        });
        Some(shared)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use compositor_vello::{Choice, VelloOptions};
    use compositor_wgpu::SharedGraphics;

    use super::{PathRenderers, vectors};

    #[test]
    fn two_windows_on_one_device_share_one_path_renderer() {
        let graphics = SharedGraphics::new();
        let Ok(gpu) = graphics.open_gpu() else {
            eprintln!("skipped: no graphics device");
            return;
        };
        if compositor_vello::chosen(gpu.adapter()) != Choice::Compute {
            eprintln!("skipped: the device cannot run vello");
            return;
        }
        let paths = PathRenderers::default();
        let first_window = vectors(VelloOptions::default(), paths.clone());
        let second_window = vectors(VelloOptions::default(), paths.clone());

        let rasters = [
            first_window(&gpu, [256, 256]),
            second_window(&gpu, [256, 256]),
        ];

        let held = paths
            .held
            .lock()
            .expect("no test panicked holding the lock");
        let shared = held
            .as_ref()
            .and_then(|entry| entry.renderer.upgrade())
            .expect("the windows' path renderer is alive");
        assert_eq!(
            Arc::strong_count(&shared),
            rasters.len() + 1,
            "both windows draw through the one path renderer"
        );
    }
}
