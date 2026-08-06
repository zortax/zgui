//! Background work, driven through a whole application with no window.
//!
//! The unit tests in `zgui-reactive` call `flush` by hand, which proves the executor works but not
//! that anything *asks* it to. That question only has an answer here: work finishing on a worker
//! thread has to travel the whole wake edge — the composite waker, the frame gate, the platform's
//! wake reason, the window's redraw request — before a frame runs and the value reaches the
//! screen. This drives that path over buffers.
//!
//! Two things are asserted, and the second matters as much as the first. The result must arrive;
//! and waiting for it must cost frames proportional to the work, not to the waiting, because a
//! wake edge that spins is a wake edge that keeps a laptop's fans on while a window sits idle.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use zgui::platform::{AppHandler, PlatformError, Surface};
use zgui::prelude::*;
use zgui::runtime::AppError;

/// How long the pretend request takes.
const WORK: Duration = Duration::from_millis(120);

/// What the loaded text ended up as, read after the application has stopped.
static LOADED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// How many closures the worker thread posted back to the UI thread.
static POSTED: AtomicUsize = AtomicUsize::new(0);

/// A view that loads something on the way in and shows it when it arrives.
#[component]
fn Loader() -> impl IntoView {
    let text = RwSignal::new(String::from("waiting"));

    spawn(async move {
        // `blocking` rather than `background`: this is a synchronous call that parks a thread,
        // which is exactly the case the UI thread must not be used for.
        let loaded = blocking(|| {
            std::thread::sleep(WORK);
            "loaded".to_string()
        })
        .await;
        // Recorded here, on the UI thread, so the assertion after the application has stopped is
        // about what a frame actually saw rather than about what a worker produced.
        LOADED.lock().unwrap().clone_from(&loaded);
        text.set(loaded);
    });

    // A thread that knows nothing about signals, reporting through a `Ui` handle.
    let ui = ui();
    std::thread::spawn(move || {
        std::thread::sleep(WORK / 2);
        ui.post(|| {
            POSTED.fetch_add(1, Ordering::Relaxed);
        });
    });

    view! {
        column(class = "loader") {
            text(class = "loader__text") {{move || text.get()}}
        }
    }
}

const SHEET: &str = css!(
    ":root { background-color: #101216; color: #ffffff; font-family: sans-serif }
     .loader { padding: 16px }"
);

/// A renderer that draws nowhere; the pipeline above it is the real thing.
#[derive(Debug, Default)]
struct Nowhere {
    /// Where glyph tiles go, so the upload path still runs.
    atlas: zgui::atlas::MemorySink,
    /// What it was pointed at.
    target: Option<zgui::render::RenderTarget>,
}

impl zgui::render::Renderer for Nowhere {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        zgui::render::RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: zgui::render::RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<zgui::render::RenderTarget> {
        self.target
    }

    fn register_external(
        &mut self,
        _texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        zgui::render::TextureHandle(0)
    }

    fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::default()
    }

    fn draw(
        &mut self,
        _scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.atlas
    }
}

/// Runs the application over buffers for long enough that the work finishes.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(4);
    harness.reset_counts();

    // Real time, because the work is on a real thread: the harness's virtual clock does not move
    // it along. Each turn parks rather than spins, which is what makes the frame count meaningful.
    let started = std::time::Instant::now();
    while started.elapsed() < WORK * 4 {
        harness.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.settle(2);

    let frames = harness.redraws_requested();
    assert!(
        frames <= 12,
        "waiting for a worker cost {frames} redraw requests, which is a spin rather than a park"
    );
    harness.assert_park_invariant();
    harness.shut_down();
    Ok(())
}

#[test]
fn work_finishing_on_a_worker_thread_reaches_the_screen_without_spinning() {
    app()
        .with_title("async")
        .with_size(320.0, 200.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
            let mut renderer = Nowhere::default();
            zgui::render::Renderer::configure(&mut renderer, target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
        }))
        .run_on(buffers, || view! { Loader() })
        .expect("the application ran");

    assert_eq!(
        LOADED.lock().unwrap().as_str(),
        "loaded",
        "the background result never reached a frame"
    );
    assert_eq!(
        POSTED.load(Ordering::Relaxed),
        1,
        "the closure a foreign thread posted never ran on the UI thread"
    );
}
