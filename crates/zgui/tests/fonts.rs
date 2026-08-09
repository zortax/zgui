//! What a `Fonts` promises: reachable from a component, callable on the UI thread, and stable
//! face handles for as long as it lives.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zgui::app::Fonts;
use zgui::platform::{AppHandler, PlatformError, Surface};
use zgui::prelude::*;
use zgui::runtime::AppError;
use zgui::view::Ident;
use zgui_text::FontSource;
use zgui_text_parley::LineRequest;

/// A renderer that draws nowhere.
#[derive(Debug, Default)]
struct Nowhere {
    /// Where the glyph tiles go, so that the upload path runs for real.
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

/// Drives an application over buffers for a few turns and stops.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(4);
    harness.shut_down();
    Ok(())
}

/// The faces this test ships, so that nothing depends on what the machine has.
fn shipped() -> Fonts {
    let fonts = Fonts::shipped_only();
    let path = format!(
        "{}/../zgui-text-parley/tests/fonts/NotoSans-Regular.ttf",
        env!("CARGO_MANIFEST_DIR")
    );
    let data = std::fs::read(&path).unwrap_or_else(|error| panic!("reading {path}: {error}"));
    fonts
        .register(Arc::new(data), Some("Test Face"))
        .expect("the shipped face registers");
    fonts
}

/// A `Fonts` clone answers all three of its questions from another thread.
#[test]
fn a_clone_answers_from_any_thread() {
    let fonts = shipped();
    let other = fonts.clone();
    let worker = std::thread::spawn(move || {
        // All three seams, on a thread that is not the one that built the collection.
        let _metrics = other.metrics();
        let _raster = other.raster();
        let mut shaper = other.shaper();
        let families = [Ident::new("Test Face")];
        shaper
            .shape_line(
                "hello",
                &LineRequest {
                    families: &families,
                    weight: 400,
                    italic: false,
                    size_device_px: 16.0,
                    letter_spacing: 0.0,
                    ligatures: true,
                },
            )
            .len()
    });
    assert_eq!(worker.join().expect("no panic"), 1);

    // And the original still answers.
    let _ = fonts.metrics();
    let _ = fonts.shaper();
}

/// A face handle keeps its meaning for as long as the collection does.
#[test]
fn a_face_handle_is_stable_for_the_life_of_the_collection() {
    let fonts = shipped();
    let mut shaper = fonts.shaper();
    let families = [Ident::new("Test Face")];
    let request = LineRequest {
        families: &families,
        weight: 400,
        italic: false,
        size_device_px: 16.0,
        letter_spacing: 0.0,
        ligatures: true,
    };
    let before = shaper.shape_line("x", &request)[0].face;

    // Registering another family issues new handles and withdraws none.
    let path = format!(
        "{}/../zgui-text-parley/tests/fonts/NotoSansArabic-Regular.ttf",
        env!("CARGO_MANIFEST_DIR")
    );
    let data = std::fs::read(&path).expect("the Arabic face is shipped with the text tests");
    fonts
        .register(Arc::new(data), Some("Other Face"))
        .expect("registers");

    let after = shaper.shape_line("x", &request)[0].face;
    assert_eq!(before, after, "the handle names the same face as before");
    assert!(
        fonts
            .shaper()
            .fonts()
            .face(before)
            .is_some_and(|record| record.family == Ident::new("Test Face")),
        "and the collection still resolves it"
    );
}

/// The application provides its faces above every window, so a component resolves them.
#[test]
fn a_component_resolves_the_applications_faces() {
    static RESOLVED: AtomicBool = AtomicBool::new(false);
    static SHAPED: AtomicBool = AtomicBool::new(false);

    app()
        .with_title("fonts")
        .with_size(320.0, 200.0)
        .with_fonts(shipped())
        .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
            let mut renderer = Nowhere::default();
            zgui::render::Renderer::configure(&mut renderer, target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
        }))
        .run_on(buffers, || {
            let fonts = use_context::<Fonts>().expect("the application provides its faces");
            RESOLVED.store(true, Ordering::SeqCst);
            let families = [Ident::new("Test Face")];
            let runs = fonts.shaper().shape_line(
                "hi",
                &LineRequest {
                    families: &families,
                    weight: 400,
                    italic: false,
                    size_device_px: 16.0,
                    letter_spacing: 0.0,
                    ligatures: true,
                },
            );
            SHAPED.store(!runs.is_empty(), Ordering::SeqCst);
            view! { column { text {"x"} } }
        })
        .expect("the application ran");

    assert!(RESOLVED.load(Ordering::SeqCst), "the context resolved");
    assert!(
        SHAPED.load(Ordering::SeqCst),
        "and the shaper it handed back drew the application's own face"
    );
}

/// The application's own setup runs after the faces are provided, so it can resolve them too.
#[test]
fn the_applications_own_setup_resolves_the_faces() {
    static SEEN: AtomicBool = AtomicBool::new(false);

    app()
        .with_title("fonts")
        .with_size(320.0, 200.0)
        .with_fonts(shipped())
        .with_context(|| {
            SEEN.store(use_context::<Fonts>().is_some(), Ordering::SeqCst);
        })
        .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
            let mut renderer = Nowhere::default();
            zgui::render::Renderer::configure(&mut renderer, target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
        }))
        .run_on(buffers, || view! { column { text {"x"} } })
        .expect("the application ran");

    assert!(SEEN.load(Ordering::SeqCst));
}
