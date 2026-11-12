//! A callback surface that stopped asking for frames, invalidated from another thread.
//!
//! The renderer draws on a real device, so the window's renderer is an offscreen `WgpuRenderer`.
//! A machine without a device prints why on standard error and skips.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use zgui_geom::{Scale, Size};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_render::RenderTarget;
use zgui_render_wgpu::{Builder, wgpu};
use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{BuildCx, IntoView, View};
use zgui_wgpu::{SurfaceElementExt, SurfaceInvalidator, SurfaceRenderCx, SurfaceRenderer};

/// A renderer that counts its renders and never asks for another frame.
struct Counting {
    /// How many times `render` ran, shared with the assertions.
    renders: Arc<AtomicUsize>,
}

impl SurfaceRenderer for Counting {
    fn render(&mut self, _cx: &mut SurfaceRenderCx<'_>) {
        self.renders.fetch_add(1, Ordering::Relaxed);
    }
}

/// Returns `true` where this machine has a device the test can draw on.
fn available() -> bool {
    let target = RenderTarget::new(Size::new(8, 8), Scale::new(1.0));
    match Builder::new().offscreen(target, wgpu::TextureFormat::Rgba8Unorm, false) {
        Ok(_) => true,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            false
        }
    }
}

/// An application with one callback surface bound to `invalidator`, drawn on an offscreen device.
fn app(renders: &Arc<AtomicUsize>, invalidator: &SurfaceInvalidator) -> Harness<Runtime> {
    let renders = Arc::clone(renders);
    let invalidator = invalidator.clone();
    let handler = App::new()
        .with_title("invalidate")
        .with_size(400.0, 300.0)
        .with_stylesheet(".pane { width: 40px; height: 30px }")
        .with_renderer(Box::new(
            |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let renderer = Builder::new()
                    .offscreen(target, wgpu::TextureFormat::Rgba8Unorm, false)
                    .map_err(AppError::GpuUnavailable)?;
                Ok(Box::new(renderer) as Box<dyn zgui_render::Renderer>)
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| Arc::new(zgui_testkit_scene::MonoRaster::new())))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::surface()
                    .class("pane")
                    .renderer_with_invalidator(
                        Counting {
                            renders: Arc::clone(&renders),
                        },
                        &invalidator,
                    )
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.app_mut().windows_mut()[0].install_embed_host(Box::new(zgui_wgpu::WgpuSurfaces::new()));
    harness
}

#[test]
fn an_invalidator_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync + Clone>() {}
    assert_send_sync::<SurfaceInvalidator>();
}

#[test]
fn an_invalidated_renderer_renders_once_more_and_only_then() {
    if !available() {
        return;
    }
    let renders = Arc::new(AtomicUsize::new(0));
    let invalidator = SurfaceInvalidator::new();
    let mut app = app(&renders, &invalidator);

    run(&mut app);
    assert_eq!(renders.load(Ordering::Relaxed), 1, "the first frame draws");

    run(&mut app);
    assert_eq!(
        renders.load(Ordering::Relaxed),
        1,
        "a renderer that asked for no frame is not called again"
    );

    let worker = invalidator.clone();
    std::thread::spawn(move || worker.invalidate())
        .join()
        .expect("the worker thread finishes");
    assert!(
        app.platform().has_pending_wakes(),
        "the invalidation woke the frame loop"
    );
    run(&mut app);
    assert_eq!(
        renders.load(Ordering::Relaxed),
        2,
        "the invalidation owed exactly one render"
    );

    run(&mut app);
    assert_eq!(
        renders.load(Ordering::Relaxed),
        2,
        "the invalidation is spent once taken"
    );
}

/// Runs the loop for a quarter second of virtual time, so a frame the loop holds back for pacing
/// also runs.
fn run(app: &mut Harness<Runtime>) {
    app.run_for(Duration::from_millis(250), Duration::from_millis(4));
}
