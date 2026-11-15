//! One path renderer per graphics device, however many windows there are.

pub mod cache;
pub mod registry;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use vello::{AaSupport, Renderer, RendererOptions};
use zgui_render_wgpu::Gpu;

pub use crate::device::registry::for_gpu;

/// A path renderer, and the fixed video memory it holds.
///
/// It is behind a lock because one of these serves every window on a device: the renderer owns
/// fixed buffers measured in the *hundreds* of megabytes, and one per window would spend a device's
/// whole budget on having several copies of the same thing. It is `Send` but not `Sync`, so the lock
/// is what makes sharing it expressible at all.
pub struct SharedRenderer {
    /// The renderer.
    renderer: Mutex<Renderer>,
    /// What it costs in video memory, as the device's own allocator reported it.
    ///
    /// Measured rather than guessed at, and measured across the *first rasterisation* rather than
    /// across construction: building the renderer compiles shaders, and the buffers that dominate
    /// the figure are allocated the first time it is actually asked to draw. It is a fixed cost that
    /// does not scale with anything and is far larger than everything that does, so a budget that
    /// folded it into one total would hide whichever of the two was really being spent.
    fixed: AtomicU64,
    /// Whether that measurement has been taken.
    settled: AtomicBool,
}

impl std::fmt::Debug for SharedRenderer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedRenderer")
            .field("fixed", &self.fixed_bytes())
            .finish_non_exhaustive()
    }
}

impl SharedRenderer {
    /// Builds a renderer on `gpu`, with its compiled-pipeline blob if the device keeps one.
    ///
    /// # Errors
    ///
    /// The renderer's own construction error, which on a device without compute shaders or without
    /// writable storage textures is where the shortfall is reported.
    pub fn new(gpu: &Gpu) -> Result<Self, vello::Error> {
        let cache = cache::load(gpu);
        let before = allocated(gpu);
        let renderer = Renderer::new(
            gpu.device(),
            RendererOptions {
                use_cpu: false,
                // Analytic area coverage, and nothing else built beside it.
                //
                // Each variant is a separate compilation of the largest shader in the pipeline, and
                // this construction is the third of a second a window pays the first time anything
                // it draws needs curves. The multisampled alternative was kept buildable so the two
                // could be compared on the content that provokes conflation — overlapping strokes,
                // a rounded icon, a self-intersecting path — and they were: worst 48 over 845
                // pixels, which is the outlines of four shapes at an edge level and not a seam. The
                // comparison is settled, so the shader is not built.
                antialiasing_support: AaSupport {
                    area: true,
                    msaa8: false,
                    msaa16: false,
                },
                num_init_threads: None,
                pipeline_cache: cache.handle().cloned(),
            },
        )?;
        cache.store();
        let fixed = allocated(gpu).saturating_sub(before);
        Ok(Self {
            renderer: Mutex::new(renderer),
            fixed: AtomicU64::new(fixed),
            settled: AtomicBool::new(false),
        })
    }

    /// Runs `rasterise`, and the first time it is called takes the difference the device's own
    /// allocator reports across it as the renderer's fixed footprint.
    ///
    /// Around the call rather than around construction, and around the *first* call rather than
    /// every one, because that is where the buffers appear and nothing of ours is allocated inside
    /// it — so the difference is attributable rather than merely correlated.
    pub fn measuring<T>(&self, gpu: &Gpu, rasterise: impl FnOnce() -> T) -> T {
        if self.settled.load(Ordering::Relaxed) {
            return rasterise();
        }
        let before = allocated(gpu);
        let value = rasterise();
        let after = allocated(gpu);
        self.fixed
            .fetch_add(after.saturating_sub(before), Ordering::Relaxed);
        self.settled.store(true, Ordering::Relaxed);
        value
    }

    /// The renderer itself, for the length of one rasterisation.
    ///
    /// # Panics
    ///
    /// Panics if a previous holder panicked while rasterising, which would leave the renderer's own
    /// buffers in an unknown state; continuing from one is how a corrupt encoding reaches a driver.
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Renderer> {
        self.renderer.lock().expect("the path renderer is usable")
    }

    /// The fixed video memory it holds, independent of what is drawn.
    ///
    /// Two measurements added together: what building it allocated, and what its first rasterisation
    /// allocated. The second is much the larger and is why the figure is not final until a frame has
    /// actually been drawn — the buffers that dominate it do not exist before then.
    pub fn fixed_bytes(&self) -> u64 {
        self.fixed.load(Ordering::Relaxed)
    }
}

/// What the device's allocator says is currently handed out, or zero where it keeps no report.
fn allocated(gpu: &Gpu) -> u64 {
    gpu.device()
        .generate_allocator_report()
        .map_or(0, |report| report.total_allocated_bytes)
}

/// A handle to the renderer shared by everything on one device.
pub type Shared = Arc<SharedRenderer>;
