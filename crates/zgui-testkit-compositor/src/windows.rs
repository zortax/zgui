//! Renderers that present into windows on the desktop, all on one shared device.

use compositor_api::Renderer;
use compositor_types::RenderTarget;
use compositor_wgpu::{PresentPacing, SharedGraphics, wgpu};

use crate::Unavailable;

/// What builds the renderers of a test's windows, each presenting into its window's surface.
///
/// Every renderer it builds draws on one device, as an application's windows do.
pub struct Windows {
    graphics: SharedGraphics,
}

/// When a window's frames are handed to the platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pacing {
    /// The display paces presentation.
    Display,
    /// The platform paces frames itself and is told before each frame is presented.
    Platform,
}

impl Windows {
    /// Returns a builder that has opened no device yet.
    pub fn new() -> Self {
        Self {
            graphics: SharedGraphics::new(),
        }
    }

    /// Returns a renderer presenting into the window `handles` name, configured for `target`.
    ///
    /// `pre_present` is called before every frame is presented.
    ///
    /// # Errors
    ///
    /// Fails when no adapter presents to the window.
    pub fn renderer(
        &self,
        target: RenderTarget,
        handles: impl Into<wgpu::SurfaceTarget<'static>> + Clone,
        pacing: Pacing,
        pre_present: Option<Box<dyn Fn(RenderTarget) + Send + Sync>>,
    ) -> Result<Box<dyn Renderer>, Unavailable> {
        let pacing = match pacing {
            Pacing::Display => PresentPacing::Display,
            Pacing::Platform => PresentPacing::Platform,
        };
        let mut renderer = self
            .graphics
            .renderer_for_surface(target.size, target.opaque, handles, pacing, pre_present)
            .map_err(|error| Unavailable::window(&error))?;
        renderer.configure(target);
        Ok(Box::new(renderer))
    }
}

impl Default for Windows {
    fn default() -> Self {
        Self::new()
    }
}
