//! What a measured window draws through: no device, this machine's device offscreen, or a window
//! on the desktop.

use std::path::PathBuf;
use std::sync::Arc;

use compositor_api::{Compositor, CompositorClient, Renderer};
use compositor_types::{
    BufferAge, Frame, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    VectorBackend,
};
use compositor_vello::VelloOptions;
use compositor_wgpu::{Gpu, WgpuRenderer, wgpu};
use zgui::platform::{PlatformError, Surface};
use zgui::runtime::AppError;

/// A renderer that accepts a frame and does nothing with it, so the numbers are the CPU's.
///
/// It reports what the device renderer reports, so the compositor builds each frame as it does
/// for a device:
///
/// - The target is kept once a frame was presented, also across a change of size.
/// - Composed pixels can be moved.
/// - Lowered chunks stay resident.
#[derive(Default)]
pub struct NullRenderer {
    /// The surface it was configured for.
    target: Option<RenderTarget>,
    /// Whether the target holds a presented frame of the current scale and opacity.
    kept: bool,
}

impl NullRenderer {
    /// Returns a renderer configured for `target`.
    pub fn new(target: RenderTarget) -> Self {
        Self {
            target: Some(target),
            kept: false,
        }
    }
}

impl Renderer for NullRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities {
            can_shift: true,
            resident_chunks: true,
            ..RenderCapabilities::MINIMAL
        }
    }

    fn configure(&mut self, target: RenderTarget) {
        // A change of scale or opacity changes what every kept pixel means. A change of size
        // keeps the picture.
        if self
            .target
            .is_some_and(|held| held.scale != target.scale || held.opaque != target.opaque)
        {
            self.kept = false;
        }
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn target_age(&self) -> BufferAge {
        if self.kept {
            BufferAge::Kept
        } else {
            BufferAge::Unknown
        }
    }

    fn draw(&mut self, _frame: &Frame) -> FrameOutcome {
        self.kept = true;
        FrameOutcome::Presented(FrameStats::default())
    }

    fn memory(&self) -> MemoryReport {
        MemoryReport::default()
    }
}

/// Opens this machine's device drawing into an offscreen texture for `target`, with vello
/// filling paths.
///
/// # Errors
///
/// Fails when no adapter opens a device.
pub fn device(target: RenderTarget) -> Result<WgpuRenderer, compositor_types::GpuUnavailable> {
    let mut gpu =
        compositor_wgpu::Builder::new().offscreen(target.size, wgpu::TextureFormat::Bgra8Unorm)?;
    attach_vello(&mut gpu);
    gpu.configure(target);
    Ok(gpu)
}

/// Gives `renderer` vello as its path rasteriser, with its compiled pipelines kept where zgui
/// keeps its other pipelines.
fn attach_vello(renderer: &mut WgpuRenderer) {
    let options = VelloOptions {
        adapter: None,
        cache_directory: pipeline_cache_directory(),
    };
    renderer.set_vector_factory(
        Box::new(move |gpu: &Arc<Gpu>, size| {
            compositor_vello::for_device(gpu.adapter(), gpu.device(), gpu.queue(), size, &options)
        }),
        VectorBackend::Compute,
        "vello",
    );
}

/// Returns the directory compiled vello pipelines are kept in: the one `ZGUI_CACHE_DIR` names,
/// else `zgui/pipelines` in the user's cache directory, or `None` when there is no such directory.
fn pipeline_cache_directory() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("ZGUI_CACHE_DIR") {
        return Some(PathBuf::from(named));
    }
    let home = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    Some(home.join("zgui").join("pipelines"))
}

/// Reports whether this run asks for its GPU passes to be timed, with `ZGUI_GPU_TIMING` or the
/// compositor's own variable.
pub fn gpu_timing_requested() -> bool {
    std::env::var_os("ZGUI_GPU_TIMING").is_some_and(|value| !value.is_empty() && value != "0")
        || compositor_wgpu::gpu_timing_requested()
}

/// Opens this machine's device for the window `surface` is, wired as an application's window is.
///
/// A platform that paces its own frames is told about every present and decides when a composed
/// frame is handed over. Elsewhere the platform is told only when `ZGUI_PRESENT_PACING=notify`
/// asks for it.
///
/// # Errors
///
/// Fails when the window offers no handles a graphics API can draw into. The renderer fails to
/// build when no adapter presents to the window.
pub fn window(
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<compositor_api::RendererFactory, AppError> {
    let handles = Arc::clone(surface).gpu_shared().ok_or_else(|| {
        AppError::Platform(PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        ))
    })?;
    let surface = Arc::clone(surface);
    Ok(Box::new(move || {
        let renderer = presenting(&surface, handles, target)?;
        Ok(Box::new(renderer) as Box<dyn Renderer>)
    }))
}

/// Opens a renderer presenting into `surface` through `handles`, with vello filling paths.
fn presenting(
    surface: &Arc<dyn Surface>,
    handles: Arc<dyn zgui::platform::GpuSurface>,
    target: RenderTarget,
) -> Result<WgpuRenderer, compositor_types::GpuUnavailable> {
    let platform = matches!(
        surface.present_pacing(),
        zgui::platform::PresentPacing::Platform
    );
    let notify =
        platform || matches!(std::env::var("ZGUI_PRESENT_PACING"), Ok(v) if v.trim() == "notify");
    let mut builder = compositor_wgpu::Builder::new();
    if platform {
        builder = builder.with_present_pacing(compositor_wgpu::PresentPacing::Platform);
    }
    if notify {
        let presented = Arc::clone(surface);
        builder = builder.with_pre_present(Box::new(move |target: RenderTarget| {
            let size = zgui::geom::Size::new(
                i32::try_from(target.size[0]).unwrap_or(i32::MAX),
                i32::try_from(target.size[1]).unwrap_or(i32::MAX),
            );
            presented.pre_present_frame(size, f64::from(target.scale));
        }));
    }
    let mut renderer = builder.for_surface(target.size, target.opaque, handles)?;
    if platform {
        let gate = Arc::clone(surface);
        renderer.set_present_gate(Box::new(move || gate.present_now()));
    }
    attach_vello(&mut renderer);
    renderer.configure(target);
    Ok(renderer)
}

/// Returns a compositor for a measured document that draws no text.
#[must_use]
pub fn compositor() -> Box<dyn Compositor> {
    Box::new(compositor_core::CompositorImpl::new())
}

/// Returns `compositor` behind the driver `ZGUI_DRIVER` selects, see
/// [`zgui_testkit_compositor::driver`].
pub fn client(compositor: Box<dyn Compositor>) -> Box<dyn CompositorClient> {
    zgui_testkit_compositor::driver(compositor)
}
