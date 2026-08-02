//! The renderer, wrapped so that the two ends of a frame's device work are timestamped.
//!
//! [`Renderer::draw`] is the call that acquires a swap-chain image, records the command buffers,
//! submits them and presents. Marking its entry and its return therefore separates the framework's
//! own work — everything before the entry — from the time spent waiting on the display, which is
//! what a presentation mode decides and no amount of faster painting can shorten.

use std::sync::Arc;
use std::time::Instant;

use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::{PlatformError, Surface};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_render_wgpu::{Builder, WgpuRenderer};
use zgui_runtime::AppError;
use zgui_scene::Scene;

use crate::tape::Shared;

/// A renderer that records when each frame's device work began and ended.
pub(crate) struct Timed {
    /// What actually draws.
    inner: WgpuRenderer,
    /// Where the moments go.
    tape: Shared,
    /// The extent the surface was last pointed at.
    extent: (u32, u32),
}

impl Timed {
    /// Wraps `inner`, recording into `tape`.
    fn new(inner: WgpuRenderer, tape: Shared) -> Self {
        Self {
            inner,
            tape,
            extent: (0, 0),
        }
    }

    /// Copies the composed target off the device and writes it as a binary PPM.
    fn snapshot(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let pixels = self.inner.read_composed();
        let size = pixels.size();
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        write!(file, "P6\n{} {}\n255\n", size.width, size.height)?;
        let mut row = Vec::with_capacity((size.width * 3) as usize);
        for y in 0..size.height {
            row.clear();
            for x in 0..size.width {
                let [r, g, b, _] = pixels.rgba(x, y);
                row.extend_from_slice(&[r, g, b]);
            }
            file.write_all(&row)?;
        }
        file.flush()
    }
}

impl Renderer for Timed {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        let at = Instant::now();
        self.extent = (
            target.size.width.max(0) as u32,
            target.size.height.max(0) as u32,
        );
        self.tape.borrow_mut().at(
            at,
            "gpu.cfg0",
            format!("{}x{}@{}", self.extent.0, self.extent.1, target.scale.get()),
        );
        self.inner.configure(target);
        self.tape.borrow_mut().now("gpu.cfg1", "");
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let began = Instant::now();
        self.tape.borrow_mut().at(
            began,
            "gpu.draw0",
            format!(
                "{}x{} full={} rects={}",
                self.extent.0,
                self.extent.1,
                damage.is_full(),
                damage.rects().len()
            ),
        );
        let outcome = self.inner.draw(scene, damage);
        let detail = match outcome {
            FrameOutcome::Presented(stats) => {
                format!(
                    "presented px={} calls={}",
                    stats.damage_px, stats.draw_calls
                )
            }
            FrameOutcome::Skipped(reason) => format!("skipped {reason:?}"),
            FrameOutcome::Recovered => "recovered".to_owned(),
            other => format!("{other:?}"),
        };
        self.tape.borrow_mut().now("gpu.draw1", detail);

        if let Ok(path) = std::env::var("ZGUI_FEEL_SHOT") {
            let path = std::path::PathBuf::from(path);
            if !path.exists() && matches!(outcome, FrameOutcome::Presented(_)) {
                let _ = self.snapshot(&path);
            }
        }
        outcome
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        self.inner.texture_sink()
    }
}

/// Opens this machine's graphics device for `surface` and wraps it.
///
/// # Errors
///
/// Returns [`AppError::Platform`] when the window offers no handles a graphics API can draw into,
/// or when no adapter on this machine can present to it.
pub(crate) fn build(
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
    tape: Shared,
) -> Result<Box<dyn Renderer>, AppError> {
    let Some(handles) = Arc::clone(surface).gpu_shared() else {
        return Err(AppError::Platform(PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        )));
    };
    let builder = Builder::new();
    let drawable = builder
        .instance()
        .create_surface(handles)
        .map_err(|error| PlatformError::Backend(error.to_string()))?;
    let inner = builder.for_surface(target, drawable)?;
    Ok(Box::new(Timed::new(inner, tape)))
}
