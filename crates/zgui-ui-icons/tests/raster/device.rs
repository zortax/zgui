//! A graphics device under the application, drawing to a texture instead of to a window.
//!
//! It is the production stack minus the compositor: the same builder, the same format rules, and
//! the same rasteriser selection an opened window goes through. Only the destination differs, and
//! it differs because a window's surface belongs to the compositor the instant it is presented and
//! cannot be read back.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use zgui::platform::Surface;
use zgui::render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, WgpuRenderer, wgpu};

use crate::raster::script::{self, Recorded};

/// Serialises every fixture in this binary onto the device.
///
/// A program has one graphics device and one reactive runtime per thread; these fixtures are the
/// only thing that would ask for several of either at once.
pub fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|held| held.into_inner())
}

/// Every frame of one run, in the order they were drawn.
pub type Log = Arc<Mutex<Vec<Recorded>>>;

/// The real renderer, with each frame's readbacks recorded beside it.
struct Recording {
    /// The renderer the application draws through.
    renderer: WgpuRenderer,
    /// What each frame produced.
    log: Log,
    /// The page's own background, which a scissored repaint is replayed over.
    background: [u8; 4],
}

impl Renderer for Recording {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        self.renderer.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        Renderer::configure(&mut self.renderer, target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.renderer.target()
    }

    fn draw(
        &mut self,
        scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        let recorded = script::record(&mut self.renderer, scene, self.background);
        let outcome = zgui::render::FrameOutcome::Presented(zgui::render::FrameStats {
            vector_passes: recorded.vector_passes,
            ..Default::default()
        });
        self.log
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(recorded);
        outcome
    }

    fn register_external(
        &mut self,
        texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        self.renderer.register_external(texture)
    }

    fn release_external(&mut self, handle: zgui::render::TextureHandle) {
        self.renderer.release_external(handle);
    }

    fn memory(&self) -> zgui::render::MemoryReport {
        self.renderer.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.renderer.texture_sink()
    }
}

/// Builds the renderer an application under test draws through, writing every frame into `log`.
///
/// Answers `None` from the factory's own error channel when this machine has no adapter that can
/// run either rasteriser, and says so out loud: a fixture that silently passed on a machine with no
/// device would be the one thing these assertions exist to rule out.
pub fn factory(
    log: &Log,
    background: [u8; 4],
) -> impl FnMut(&Arc<dyn Surface>, RenderTarget) -> Result<Box<dyn Renderer>, zgui::runtime::AppError>
+ use<> {
    let log = Arc::clone(log);
    move |_surface, target| {
        let mut renderer = Builder::new()
            .offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
            .map_err(zgui::runtime::AppError::GpuUnavailable)?;
        // The same call a window makes, rather than a second copy of it: a fixture that wired
        // its own rasteriser up would stay green through the wiring being deleted.
        zgui_render_vector_vello::attach(&mut renderer, target.size);
        assert!(
            renderer.has_vector_raster(),
            "the selection handed back no rasteriser, so nothing below would draw a path"
        );
        Ok(Box::new(Recording {
            renderer,
            log: Arc::clone(&log),
            background,
        }) as Box<dyn Renderer>)
    }
}

/// Whether this machine has a graphics device these fixtures can run on at all.
pub fn available() -> bool {
    static ANSWER: OnceLock<bool> = OnceLock::new();
    *ANSWER.get_or_init(|| {
        let target = RenderTarget::new(zgui::geom::Size::new(64, 64), zgui::geom::Scale::new(1.0));
        match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
            Ok(_) => true,
            Err(failure) => {
                eprintln!("skipped: no usable graphics device ({failure})");
                false
            }
        }
    })
}
