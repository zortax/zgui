//! The renderers that draw windows into offscreen textures and read every presented image back.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use iceblink::{
    BufferAge, Frame, FrameOutcome, MemoryReport, Presenter, RenderCapabilities, RenderTarget,
    Renderer, SubpixelOrder, TargetPoolReport, VectorBackend, VectorStatus,
};
use iceblink_vello::VelloOptions;
use iceblink_wgpu::{Builder, Pixels, WgpuRenderer, wgpu};
use zgui_runtime::RendererFactory;

use crate::app::compositor::{PathRenderers, latency_marks, pipeline_directory, vectors};

/// The image an offscreen window presented last, readable while the window's renderer draws
/// elsewhere.
#[derive(Clone, Default)]
pub struct Presented(Arc<Mutex<Option<Pixels>>>);

impl Presented {
    /// Takes the image read back after the last frame the renderer drew, or `None` when it drew
    /// no frame since the last take.
    pub fn take(&self) -> Option<Pixels> {
        self.lock().take()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Pixels>> {
        self.0.lock().unwrap_or_else(|held| held.into_inner())
    }
}

impl core::fmt::Debug for Presented {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Presented")
    }
}

/// Returns the factory of `iceblink-wgpu` renderers that draw each window into a BGRA texture and
/// keep the image each frame presents in `presented`.
///
/// Each renderer holds its groups at `precision`, which `ZGUI_GROUP_PRECISION` overrides. A
/// renderer fails to build as [`iceblink::GpuUnavailable`] when no adapter opens a device.
pub(crate) fn factory(
    precision: Option<crate::GroupPrecision>,
    presented: Presented,
) -> RendererFactory {
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
    Box::new(move |_surface, target| {
        let (vello, paths, presented) = (vello.clone(), paths.clone(), presented.clone());
        Ok(Box::new(move || {
            let mut builder = Builder::new().with_latency_marks(latency_marks());
            if let Some(directory) = pipeline_directory() {
                builder = builder.with_pipeline_cache_dir(directory);
            }
            let mut renderer = builder
                .with_group_precision(precision)
                .offscreen(target.size, wgpu::TextureFormat::Bgra8Unorm)?;
            // Without a path rasteriser, paths and glyphs on the path route draw nothing.
            renderer.set_vector_factory(vectors(vello, paths), VectorBackend::Compute, "vello");
            renderer.configure(target);
            Ok(Box::new(Offscreen {
                renderer,
                presented,
            }) as Box<dyn Renderer>)
        }))
    })
}

/// A renderer drawing into an offscreen texture that reads back every image it presents.
struct Offscreen {
    renderer: WgpuRenderer,
    presented: Presented,
}

impl Offscreen {
    /// Reads back the image presented last, keeping the earlier one when nothing was presented.
    fn read_back(&mut self) {
        if let Some(pixels) = self.renderer.read_presented() {
            *self.presented.lock() = Some(pixels);
        }
    }
}

impl Renderer for Offscreen {
    fn capabilities(&self) -> RenderCapabilities {
        self.renderer.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.renderer.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.renderer.target()
    }

    fn target_age(&self) -> BufferAge {
        self.renderer.target_age()
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let outcome = self.renderer.draw(frame);
        self.read_back();
        outcome
    }

    fn present_held(&mut self) -> FrameOutcome {
        let outcome = self.renderer.present_held();
        self.read_back();
        outcome
    }

    fn acquire_block(&self) -> Duration {
        self.renderer.acquire_block()
    }

    fn set_subpixel_order(&mut self, order: SubpixelOrder) {
        self.renderer.set_subpixel_order(order);
    }

    fn presenter(&mut self) -> Option<&mut dyn Presenter> {
        self.renderer.presenter()
    }

    fn memory(&self) -> MemoryReport {
        self.renderer.memory()
    }

    fn target_pool(&self) -> TargetPoolReport {
        self.renderer.target_pool()
    }

    fn release_cached_targets(&mut self) -> u64 {
        self.renderer.release_cached_targets()
    }

    fn release_idle_resources(&mut self) -> u64 {
        self.renderer.release_idle_resources()
    }

    fn vector_status(&self) -> VectorStatus {
        self.renderer.vector_status()
    }

    fn extra_stats(&self, out: &mut dyn FnMut(&'static str, u64)) {
        self.renderer.extra_stats(out);
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        self.renderer.as_any()
    }

    // Surface elements find the device by downcasting to the wgpu renderer.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.renderer.as_any_mut()
    }
}
