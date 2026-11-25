//! A renderer that accepts a frame and draws nowhere.
//!
//! It exists so that a budget is the pipeline's own cost: a renderer that recorded the frames
//! would put its own serialisation into every measurement, and a real one would put a graphics
//! driver into it. Everything above it — the cascade, the box tree, layout, shaping, the paint
//! stage and the compositor — is the real thing.

use std::sync::Arc;

use iceblink::{RenderTarget, Renderer};
use zgui_platform::Surface;
use zgui_runtime::AppError;

/// Builds the renderer a window draws through.
pub fn build(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let mut renderer = zgui_testkit_compositor::NullRenderer::default();
    renderer.configure(target);
    Ok(Box::new(renderer))
}
