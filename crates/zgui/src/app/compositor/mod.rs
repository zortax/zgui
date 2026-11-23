//! The compositor every window is drawn through, the driver that runs it, and the renderers that
//! draw its frames.

mod paths;
mod window;

use std::sync::{Arc, Mutex};

use compositor_api::{Compositor, CompositorBudgets, CompositorClient};
use compositor_core::CompositorImpl;
use compositor_text::SwashRaster;
use compositor_wgpu::LatencyMarks;
use zgui_runtime::RendererFactory;
use zgui_runtime::budget::limits::ATLAS_BYTES;

pub(crate) use self::paths::{PathRenderers, vectors};
pub(crate) use self::window::factory;
use crate::app::driver::CompositorDriver;

/// Returns the compositor an application's windows draw through, drawing text in `fonts`.
///
/// It is the reference compositor, it rasterises glyphs with swash, and its atlas is held to
/// [`ATLAS_BYTES`].
pub fn compositor(fonts: &crate::app::Fonts) -> Box<dyn Compositor> {
    let mut compositor = CompositorImpl::new();
    compositor.use_font_registry(fonts.compositor_registry());
    compositor.use_glyph_raster(Arc::new(SwashRaster::new()));
    compositor.set_budgets(CompositorBudgets {
        atlas_bytes: Some(ATLAS_BYTES),
    });
    Box::new(compositor)
}

/// Returns `compositor` behind `driver`, which `ZGUI_DRIVER` overrides with `local` or
/// `threaded`.
pub(crate) fn driven(
    compositor: Box<dyn Compositor>,
    driver: CompositorDriver,
) -> Box<dyn CompositorClient> {
    // A page has one thread.
    let driver = if cfg!(target_arch = "wasm32") {
        CompositorDriver::InProcess
    } else {
        CompositorDriver::from_environment().unwrap_or(driver)
    };
    match driver {
        CompositorDriver::Threaded => {
            Box::new(compositor_thread::ThreadedCompositor::spawn(compositor).0)
        }
        CompositorDriver::InProcess => Box::new(compositor_driver::InProcess::new(
            compositor,
            compositor_driver::DriverOptions::default(),
        )),
    }
}

/// The variable that names the directory compiled pipelines are kept in.
pub const PIPELINE_DIRECTORY_VARIABLE: &str = "ZGUI_CACHE_DIR";

/// Returns the directory compiled pipelines are kept in between runs: the one
/// [`PIPELINE_DIRECTORY_VARIABLE`] names, then `zgui/pipelines` under `XDG_CACHE_HOME` or under
/// `~/.cache`.
///
/// Returns `None` when the environment names none of them.
pub fn pipeline_directory() -> Option<std::path::PathBuf> {
    if let Ok(named) = std::env::var(PIPELINE_DIRECTORY_VARIABLE) {
        return Some(named.into());
    }
    let home = std::env::var("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        })
        .ok()?;
    Some(home.join("zgui").join("pipelines"))
}

/// Returns the marks a renderer emits at the steps of a frame, recorded into zgui's latency traces.
pub fn latency_marks() -> LatencyMarks {
    Arc::new(|stage, detail| match detail {
        Some(detail) => zgui_profile::latency::note(stage, detail),
        None => zgui_profile::latency::mark(stage),
    })
}

/// Returns the factory of the renderers `make` returns, configured for each window's target.
///
/// `make` runs on the thread that draws the window's frames.
pub(crate) fn wrapping(make: crate::app::CompositorRendererMaker) -> RendererFactory {
    let make = Arc::new(Mutex::new(make));
    Box::new(move |_surface, target| {
        let make = Arc::clone(&make);
        Ok(Box::new(move || {
            let mut make = make.lock().unwrap_or_else(|held| held.into_inner());
            let mut renderer = make(target.size);
            renderer.configure(target);
            Ok(renderer)
        }))
    })
}
