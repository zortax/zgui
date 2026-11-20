//! The compositor and renderer zgui's tests draw through.
//!
//! Every zgui test that needs a compositor or a renderer takes it from [`compositor`] and
//! [`renderer`], so each test runs against whichever implementation the environment selects:
//!
//! - `ZGUI_COMPOSITOR` selects the compositor: `core`, the default, or `naive`.
//! - `ZGUI_RENDERER` selects the renderer: `capture`, the default, `wgpu`, `cpu` or `null`.
//!
//! An implementation that does not exist yet, an unknown value, and a device that cannot be
//! opened are reported as [`Unavailable`]. The factory never substitutes another implementation.
//!
//! ```
//! let compositor = zgui_testkit_compositor::compositor();
//! assert!(compositor.is_ok() || std::env::var_os("ZGUI_COMPOSITOR").is_some());
//! ```

#![forbid(unsafe_code)]

mod null;
mod select;
mod unavailable;

use compositor_api::{Compositor, Renderer};
use compositor_capture::CaptureRenderer;
use compositor_core::CompositorImpl;
use compositor_text::SwashRaster;
use std::sync::Arc;

pub use crate::null::NullRenderer;
pub use crate::select::{CompositorKind, RendererKind};
pub use crate::unavailable::Unavailable;

/// Returns the compositor `ZGUI_COMPOSITOR` selects, with verification of damage hints on and
/// glyphs rasterised through swash.
///
/// # Errors
///
/// Fails when the variable holds an unknown value or names an implementation not written yet.
pub fn compositor() -> Result<Box<dyn Compositor>, Unavailable> {
    match CompositorKind::from_environment()? {
        CompositorKind::Core => {
            let mut compositor = CompositorImpl::new();
            compositor.set_verification(true);
            compositor.use_glyph_raster(Arc::new(SwashRaster::new()));
            Ok(Box::new(compositor))
        }
        CompositorKind::Naive => Err(Unavailable::not_written("ZGUI_COMPOSITOR", "naive")),
    }
}

/// Returns the renderer `ZGUI_RENDERER` selects, drawing into an offscreen target of `size`
/// device pixels where it draws at all.
///
/// # Errors
///
/// Fails when the variable holds an unknown value, names an implementation not written yet, or
/// selects `wgpu` on a machine where no adapter opens a device.
pub fn renderer(size: [u32; 2]) -> Result<Box<dyn Renderer>, Unavailable> {
    match RendererKind::from_environment()? {
        RendererKind::Capture => Ok(Box::new(CaptureRenderer::new())),
        RendererKind::Null => Ok(Box::new(NullRenderer::default())),
        RendererKind::Wgpu => compositor_wgpu::Builder::new()
            .offscreen(size, compositor_wgpu::wgpu::TextureFormat::Rgba8Unorm)
            .map(|renderer| Box::new(renderer) as Box<dyn Renderer>)
            .map_err(|error| Unavailable::device(&error)),
        RendererKind::Cpu => Err(Unavailable::not_written("ZGUI_RENDERER", "cpu")),
    }
}
