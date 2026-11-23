//! The compositor and renderer zgui's tests draw through.
//!
//! Every zgui test that needs a compositor or a renderer takes it from [`compositor`] and
//! [`renderer`], so each test runs against whichever implementation the environment selects:
//!
//! - `ZGUI_COMPOSITOR` selects the compositor: `core`, the default, or `naive`.
//! - `ZGUI_RENDERER` selects the renderer: `capture`, the default, `wgpu`, `cpu` or `null`.
//! - `ZGUI_DRIVER` selects the driver the compositor runs behind: `local`, the default,
//!   `threaded` or `paced`, see [`driver`].
//!
//! An implementation that does not exist yet, an unknown value, and a device that cannot be
//! opened are reported as [`Unavailable`]. The factory never substitutes another implementation.
//!
//! A [`Screen`] is a recording renderer whose last frame is the whole picture on its output, for a
//! window drawing through a compositor. A [`Kept`] renderer keeps its target, so each frame it
//! records is what the compositor redraws, and an [`Aged`] renderer composes into a rotation of
//! images by their age.
//!
//! A [`Session`] drives the selected compositor as one client drawing into one output, with a
//! recording renderer under it. Each frame comes back as a [`Drawing`], the instances the frame
//! draws, read from its capture transcript.
//!
//! ```
//! let compositor = zgui_testkit_compositor::compositor();
//! assert!(compositor.is_ok() || std::env::var_os("ZGUI_COMPOSITOR").is_some());
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod aged;
mod device;
mod drawing;
mod driver;
mod kept;
mod null;
mod output;
mod screen;
mod screens;
mod select;
mod session;
mod synced;
mod tierless;
mod unavailable;
mod windows;

use compositor_api::{Compositor, Renderer};
use compositor_capture::CaptureRenderer;
use compositor_core::CompositorImpl;
use compositor_text::SwashRaster;
use std::sync::Arc;

pub use crate::aged::{Aged, Refusals};
pub use crate::device::{Composed, Device, Pixels};
pub use crate::drawing::{Drawing, Instance};
pub use crate::driver::{driver, factory, self_paced};
pub use crate::kept::{Drawn, Frames, Kept};
pub use crate::null::NullRenderer;
pub use crate::output::TestOutput;
pub use crate::screen::{Screen, Shown, drawing_of};
pub use crate::screens::{screen, screens};
pub use crate::select::{CompositorKind, DriverKind, RendererKind};
pub use crate::session::{FONTS, Session, with_fake_text};
pub use crate::tierless::tierless;
pub use crate::unavailable::Unavailable;
pub use crate::windows::{Pacing, Windows};

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

/// Returns the compositor [`compositor`] selects, with the fake text of [`with_fake_text`].
///
/// # Panics
///
/// Panics when the environment selects a compositor that cannot be provided.
pub fn fake_text_compositor() -> Box<dyn Compositor> {
    let mut compositor = compositor().unwrap_or_else(|why| panic!("no compositor: {why}"));
    with_fake_text(&mut *compositor);
    compositor
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
