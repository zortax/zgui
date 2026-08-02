//! A graphics device under the gallery, drawing to a texture instead of to a window.
//!
//! It is the production stack minus the compositor: the same builder, the same format rules and the
//! same rasteriser selection an opened window goes through. Only the destination differs, and it
//! differs because a window's surface belongs to the compositor the instant it is presented and
//! cannot be read back.

#![allow(
    dead_code,
    unreachable_pub,
    reason = "one support module serves several groups of assertions, none of which uses all of it"
)]

pub mod frame;
pub mod ink;
pub mod shot;

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use zgui::platform::Surface;
use zgui::render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, WgpuRenderer, wgpu};

use crate::device::frame::Frame;

/// Serialises every fixture in this binary onto the device.
///
/// A process has one graphics device and one reactive runtime per thread; these fixtures are the
/// only thing that would ask for several of either at once.
pub fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|held| held.into_inner())
}

/// Every frame of one run, in the order they were drawn.
pub type Log = Arc<Mutex<Vec<Frame>>>;

/// The real renderer, with each frame recorded beside it.
struct Recording {
    /// The renderer the application draws through.
    renderer: WgpuRenderer,
    /// What each frame produced.
    log: Log,
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
        // Over the whole surface, whatever the application asked for. The question is what the
        // gallery *contains*, and a scissored frame answers it only for the rectangle it redrew.
        let outcome = self.renderer.draw(scene, &zgui::bits::DamageSet::full());
        let recorded = Frame::record(&mut self.renderer, scene, &outcome);
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

/// Builds the renderer the application under test draws through, writing every frame into `log`.
///
/// # Panics
///
/// Panics if the rasteriser selection hands back nothing, because a renderer with no rasteriser
/// composites a scratch nobody wrote and every drawing in the window is empty space — which is
/// precisely the state these fixtures exist to tell apart from a drawing that failed to reach the
/// display list.
pub fn factory(
    log: &Log,
) -> impl FnMut(&Arc<dyn Surface>, RenderTarget) -> Result<Box<dyn Renderer>, zgui::runtime::AppError>
+ use<> {
    let log = Arc::clone(log);
    move |_surface, target| {
        let mut renderer = Builder::new()
            .offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
            .map_err(zgui::runtime::AppError::GpuUnavailable)?;
        // The same call a window makes, rather than a second copy of it.
        zgui_render_vector_vello::attach(&mut renderer, target.size);
        assert!(
            renderer.has_vector_raster(),
            "the selection handed back no rasteriser, so nothing below would draw a path"
        );
        Ok(Box::new(Recording {
            renderer,
            log: Arc::clone(&log),
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
