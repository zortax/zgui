//! A callback surface that stopped asking for frames, invalidated from another thread.
//!
//! The window draws on a real device through the compositor's test device. A machine without a
//! device prints why on standard error and skips.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, Runtime};
use zgui_testkit_compositor::Device;
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
    match Device::open([8, 8]) {
        Ok(_) => true,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            false
        }
    }
}

/// An application with `count` callback surfaces bound to `invalidator`, drawn on an offscreen
/// device. All surfaces count into `renders`.
fn app(
    renders: &Arc<AtomicUsize>,
    invalidator: &SurfaceInvalidator,
    count: usize,
) -> Harness<Runtime> {
    let renders = Arc::clone(renders);
    let invalidator = invalidator.clone();
    let handler = App::new()
        .with_title("invalidate")
        .with_size(400.0, 300.0)
        .with_stylesheet(".pane { width: 40px; height: 30px }")
        .with_compositor(zgui_testkit_compositor::driver(
            zgui_testkit_compositor::fake_text_compositor(),
        ))
        .with_renderer(Box::new(
            |_surface: &Arc<dyn Surface>, target: iceblink::RenderTarget| {
                Ok::<_, AppError>(zgui_testkit_compositor::factory(move || {
                    let mut device = Device::open(target.size)
                        .expect("a device opens, which the test answered beforehand");
                    iceblink::Renderer::configure(&mut device, target);
                    device
                }))
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(zgui_testkit::MonoShaper::new()))
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let mut root = zgui_elements::r#box();
            for _ in 0..count {
                root = root.child(
                    zgui_elements::surface()
                        .class("pane")
                        .renderer_with_invalidator(
                            Counting {
                                renders: Arc::clone(&renders),
                            },
                            &invalidator,
                        ),
                );
            }
            Box::new(root.into_view().build(cx))
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
    let mut app = app(&renders, &invalidator, 1);

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

#[test]
fn one_invalidator_renders_every_renderer_bound_to_it() {
    if !available() {
        return;
    }
    let renders = Arc::new(AtomicUsize::new(0));
    let invalidator = SurfaceInvalidator::new();
    let mut app = app(&renders, &invalidator, 2);

    run(&mut app);
    assert_eq!(
        renders.load(Ordering::Relaxed),
        2,
        "the first frame draws both"
    );

    invalidator.invalidate();
    invalidator.invalidate();
    run(&mut app);
    assert_eq!(
        renders.load(Ordering::Relaxed),
        4,
        "each renderer renders once more, and the two calls merge"
    );
}

/// Runs the loop for a quarter second of virtual time, so a frame the loop holds back for pacing
/// also runs.
fn run(app: &mut Harness<Runtime>) {
    app.run_for(Duration::from_millis(250), Duration::from_millis(4));
}
