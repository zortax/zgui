//! A renderer that accepts a frame and draws nowhere.
//!
//! Everything above it is the real thing — the cascade, the box tree, layout, shaping, the paint
//! stage and the compositor — because the question these fixtures ask is about dispatch and
//! geometry, and a graphics device would only add a reason for them not to run on a machine that
//! has none.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use iceblink::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
};
use zgui::platform::Surface;
use zgui::runtime::AppError;
use zgui_testkit_compositor::NullRenderer;

/// The most paths any frame has handed the path rasteriser.
///
/// A path rasteriser is built by the first frame that hands it a path, and it is then held for the
/// life of the device. So the number worth watching is the high-water mark over a whole fixture
/// rather than the last frame's: an interface that reaches for one only while a panel is opening
/// has still reached for it.
static VECTOR_ITEMS: AtomicUsize = AtomicUsize::new(0);

/// The most vector passes any frame has planned.
static VECTOR_PASSES: AtomicUsize = AtomicUsize::new(0);

/// Forgets what earlier frames carried, so one fixture's count is its own.
pub fn forget_vectors() {
    VECTOR_ITEMS.store(0, Ordering::Relaxed);
    VECTOR_PASSES.store(0, Ordering::Relaxed);
}

/// The most paths and vector passes any frame has carried since [`forget_vectors`].
pub fn vectors_drawn() -> (usize, usize) {
    (
        VECTOR_ITEMS.load(Ordering::Relaxed),
        VECTOR_PASSES.load(Ordering::Relaxed),
    )
}

/// A renderer that draws nowhere and keeps the high-water marks of the vector content of every
/// frame.
#[derive(Default)]
struct Counting {
    inner: NullRenderer,
}

impl Renderer for Counting {
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
        VECTOR_ITEMS.fetch_max(frame.paths.len(), Ordering::Relaxed);
        VECTOR_PASSES.fetch_max(frame.vector_plan.passes.len(), Ordering::Relaxed);
        self.inner.draw(frame)
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }
}

/// Builds the renderer a window draws through: one that accepts a frame and draws nowhere.
pub fn build(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<iceblink::RendererFactory, AppError> {
    Ok(zgui_testkit_compositor::factory(move || {
        let mut renderer = Counting::default();
        renderer.configure(target);
        renderer
    }))
}
