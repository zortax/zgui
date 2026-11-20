//! A renderer that accepts a frame and draws nowhere.
//!
//! Everything above it is the real thing — the cascade, the box tree, layout, shaping, the paint
//! stage and the compositor — because the question these fixtures ask is about dispatch and
//! geometry, and a graphics device would only add a reason for them not to run on a machine that
//! has none.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use compositor_api::Renderer;
use compositor_types::RenderTarget;
use zgui::platform::Surface;
use zgui::runtime::AppError;

/// The most vector items any frame's display list has carried.
///
/// A path rasteriser is built by the first frame whose display list holds one, and it is then held
/// for the life of the device. So the number worth watching is the high-water mark over a whole
/// fixture rather than the last frame's: an interface that reaches for one only while a panel is
/// opening has still reached for it.
static VECTOR_ITEMS: AtomicUsize = AtomicUsize::new(0);

/// The most vector passes any frame's display list has planned.
static VECTOR_PASSES: AtomicUsize = AtomicUsize::new(0);

/// Forgets what earlier frames carried, so one fixture's count is its own.
pub fn forget_vectors() {
    VECTOR_ITEMS.store(0, Ordering::Relaxed);
    VECTOR_PASSES.store(0, Ordering::Relaxed);
}

/// The most vector items and vector passes any frame has carried since [`forget_vectors`].
pub fn vectors_drawn() -> (usize, usize) {
    (
        VECTOR_ITEMS.load(Ordering::Relaxed),
        VECTOR_PASSES.load(Ordering::Relaxed),
    )
}

/// A probe that keeps the high-water marks of the vector content in every frame's display list.
struct Counting;

impl zgui::runtime::FrameProbe for Counting {
    fn frame_ended(&self, window: &zgui::runtime::Window) {
        let scene = window.scene();
        VECTOR_ITEMS.fetch_max(scene.primitives.vectors.len(), Ordering::Relaxed);
        VECTOR_PASSES.fetch_max(scene.pass_plan().passes.len(), Ordering::Relaxed);
    }
}

/// Builds the probe that counts what [`vectors_drawn`] reports.
pub fn counting() -> Rc<dyn zgui::runtime::FrameProbe> {
    Rc::new(Counting)
}

/// Builds the renderer a window draws through: one that accepts a frame and does nothing with it.
pub fn build(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let mut renderer = zgui_testkit_compositor::NullRenderer::default();
    renderer.configure(target);
    Ok(Box::new(renderer))
}
