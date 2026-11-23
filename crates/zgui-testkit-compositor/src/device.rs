//! A graphics device drawing offscreen, with each frame also recorded as a transcript.

use std::sync::{Arc, Mutex};

use compositor_api::Renderer;
use compositor_capture::CaptureRenderer;
use compositor_types::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, SubpixelOrder,
    VectorBackend,
};
use compositor_vello::VelloOptions;
use compositor_wgpu::{Gpu, WgpuRenderer, wgpu};

pub use compositor_wgpu::Pixels;

use crate::Unavailable;
use crate::drawing::Drawing;
use crate::screen::Shown;

/// The wgpu reference renderer drawing into an offscreen texture, with a recording renderer
/// applying every frame beside it.
///
/// After each frame the composed target is read back. Paths, and glyphs on the path route, are
/// filled by vello.
pub struct Device {
    wgpu: WgpuRenderer,
    capture: CaptureRenderer,
    shown: Shown,
    composed: Composed,
}

/// The last composed target of a [`Device`], readable after the device has been handed away.
#[derive(Clone, Default)]
pub struct Composed(Arc<Mutex<Option<Arc<Pixels>>>>);

impl Composed {
    /// Returns the composed target as the last frame left it, or `None` before the first frame.
    pub fn pixels(&self) -> Option<Arc<Pixels>> {
        self.0
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()
    }
}

impl core::fmt::Debug for Composed {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Composed")
    }
}

impl Device {
    /// Opens a device drawing into a BGRA texture of `size` device pixels.
    ///
    /// # Errors
    ///
    /// Fails when no adapter opens a device.
    pub fn open(size: [u32; 2]) -> Result<Self, Unavailable> {
        let mut wgpu = compositor_wgpu::Builder::new()
            .offscreen(size, wgpu::TextureFormat::Bgra8Unorm)
            .map_err(|error| Unavailable::device(&error))?;
        let options = VelloOptions {
            adapter: None,
            cache_directory: None,
        };
        wgpu.set_vector_factory(
            Box::new(move |gpu: &Arc<Gpu>, size| {
                compositor_vello::for_device(
                    gpu.adapter(),
                    gpu.device(),
                    gpu.queue(),
                    size,
                    &options,
                )
            }),
            VectorBackend::Compute,
            "vello",
        );
        let capture = CaptureRenderer::with_capabilities(wgpu.capabilities());
        Ok(Self {
            wgpu,
            capture,
            shown: Shown::default(),
            composed: Composed::default(),
        })
    }

    /// Returns a handle that reads what the output shows, as the last frame's transcript.
    ///
    /// The last frame draws only its damage, so the transcript holds what that frame redrew.
    pub fn shown(&self) -> Shown {
        self.shown.clone()
    }

    /// Returns a handle that reads the composed target.
    pub fn composed(&self) -> Composed {
        self.composed.clone()
    }
}

impl core::fmt::Debug for Device {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_struct("Device").finish_non_exhaustive()
    }
}

impl Renderer for Device {
    fn capabilities(&self) -> RenderCapabilities {
        self.wgpu.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.wgpu.configure(target);
        self.capture.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.wgpu.target()
    }

    fn target_age(&self) -> BufferAge {
        self.wgpu.target_age()
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        self.capture.draw(frame);
        let outcome = self.wgpu.draw(frame);
        if let Some(captured) = self.capture.last() {
            self.shown
                .set(Drawing::of(captured.drawing.clone()), self.capture.frames());
        }
        *self
            .composed
            .0
            .lock()
            .unwrap_or_else(|held| held.into_inner()) = Some(Arc::new(self.wgpu.read_composed()));
        outcome
    }

    fn memory(&self) -> MemoryReport {
        self.wgpu.memory()
    }

    fn set_subpixel_order(&mut self, order: SubpixelOrder) {
        self.wgpu.set_subpixel_order(order);
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }

    /// The wgpu renderer underneath, which a backend companion binds textures on.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.wgpu.as_any_mut()
    }
}
