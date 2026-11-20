//! GPU time per measured phase, with `ZGUI_BENCH_GPU=1 ZGUI_GPU_TIMING=1`.
//!
//! A scenario names the phase it measures with [`phase`]. The renderer reports a frame's GPU time
//! one or two frames after the frame, so [`Timed`] keeps the phase of each frame it draws by frame
//! number and files the result under that phase when it arrives. A window's last results are read
//! when its renderer drops. [`print`] writes one `GPU` line per phase.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use compositor_api::Renderer;
use compositor_types::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget,
};
use compositor_wgpu::{GpuTiming, WgpuRenderer};

use crate::scenario::band::Spread;

thread_local! {
    /// The phase frames drawn now belong to.
    static CURRENT: RefCell<Option<String>> = const { RefCell::new(None) };

    /// The results filed so far, per phase, in the order the phases first got one.
    static PHASES: RefCell<Vec<(String, Vec<GpuTiming>)>> = const { RefCell::new(Vec::new()) };
}

/// Whether this run draws on the device and times its passes.
pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("ZGUI_BENCH_GPU").is_some()
            && (zgui_render_wgpu::gpu::timing::requested()
                || compositor_wgpu::gpu_timing_requested())
    })
}

/// The phase frames belong to while this lives.
#[must_use = "the phase ends when this drops"]
pub(crate) struct Phase {
    /// The phase that was current before, put back on drop.
    previous: Option<String>,
}

impl Drop for Phase {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|current| *current.borrow_mut() = previous);
    }
}

/// Files the GPU time of each frame drawn from now until the result drops under `name`.
///
/// Does nothing unless [`enabled`].
pub(crate) fn phase(name: &str) -> Phase {
    let previous = if enabled() {
        CURRENT.with(|current| current.borrow_mut().replace(name.to_owned()))
    } else {
        None
    };
    Phase { previous }
}

/// Files `timing` under `phase`.
fn file(phase: String, timing: GpuTiming) {
    PHASES.with(|phases| {
        let mut phases = phases.borrow_mut();
        match phases.iter_mut().find(|(name, _)| *name == phase) {
            Some((_, timings)) => timings.push(timing),
            None => phases.push((phase, vec![timing])),
        }
    });
}

/// The device's renderer, with each frame's GPU time filed under the phase that drew it.
pub(crate) struct Timed {
    /// The renderer that does the work.
    inner: WgpuRenderer,
    /// The phase of each drawn frame whose result has not arrived, by frame number.
    pending: BTreeMap<u64, String>,
}

impl Timed {
    /// Wraps `inner`.
    pub(crate) fn new(inner: WgpuRenderer) -> Self {
        if !inner.gpu_timing_enabled() {
            eprintln!(
                "GPU timing was asked for, but {} has no timestamp queries",
                inner.gpu().describe()
            );
        }
        Self {
            inner,
            pending: BTreeMap::new(),
        }
    }

    /// Files every result that has arrived.
    fn collect(&mut self) {
        for timing in self.inner.take_gpu_timings() {
            let phase = self.pending.remove(&timing.frame);
            // Results arrive in frame order, so an older frame still pending got no query set.
            self.pending.retain(|frame, _| *frame > timing.frame);
            if let Some(phase) = phase {
                file(phase, timing);
            }
        }
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        if self.inner.gpu_timing_enabled() {
            self.inner.gpu().wait();
            self.collect();
        }
    }
}

impl Renderer for Timed {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn target_age(&self) -> BufferAge {
        self.inner.target_age()
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let before = self.inner.gpu_timed_frames();
        let outcome = self.inner.draw(frame);
        if let (Some(before), Some(after)) = (before, self.inner.gpu_timed_frames())
            && let Some(phase) = CURRENT.with(|current| current.borrow().clone())
        {
            for frame in before..after {
                self.pending.insert(frame, phase.clone());
            }
        }
        self.collect();
        outcome
    }

    fn present_held(&mut self) -> FrameOutcome {
        self.inner.present_held()
    }

    fn acquire_block(&self) -> core::time::Duration {
        self.inner.acquire_block()
    }

    fn set_subpixel_order(&mut self, order: compositor_types::SubpixelOrder) {
        self.inner.set_subpixel_order(order);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn target_pool(&self) -> compositor_types::TargetPoolReport {
        self.inner.target_pool()
    }

    fn release_cached_targets(&mut self) -> u64 {
        self.inner.release_cached_targets()
    }

    fn release_idle_resources(&mut self) -> u64 {
        self.inner.release_idle_resources()
    }

    fn vector_status(&self) -> compositor_types::VectorStatus {
        self.inner.vector_status()
    }

    fn extra_stats(&self, out: &mut dyn FnMut(&'static str, u64)) {
        self.inner.extra_stats(out);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.inner.as_any_mut()
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        self.inner.as_any()
    }
}

/// Prints one `GPU` line per phase filed so far, and forgets them.
///
/// The line holds the p50, p95 and maximum of the frame total in milliseconds, the frame count,
/// and the p50 of each kind of work.
pub(crate) fn print(scope: &str) {
    if !enabled() {
        return;
    }
    let phases = PHASES.with(|phases| std::mem::take(&mut *phases.borrow_mut()));
    for (phase, timings) in phases {
        let mut totals: Vec<f64> = timings.iter().map(GpuTiming::total_ms).collect();
        let total = Spread::of(&mut totals);
        let p50 = |kind: fn(&GpuTiming) -> f64| {
            let mut values: Vec<f64> = timings.iter().map(kind).collect();
            Spread::of(&mut values).p50
        };
        let untimed: u32 = timings.iter().map(|timing| timing.untimed).sum();
        println!(
            "GPU\t{scope}\t{phase}\tms\tp50={:.4};p95={:.4};max={:.4};n={}\tdraws_p50={:.4};\
             groups_p50={:.4};vectors_p50={:.4};blit_p50={:.4};untimed_passes={untimed}",
            total.p50,
            total.p95,
            total.max,
            total.samples,
            p50(|timing| timing.draws_ms),
            p50(|timing| timing.groups_ms),
            p50(|timing| timing.vectors_ms),
            p50(|timing| timing.blit_ms),
        );
    }
}
