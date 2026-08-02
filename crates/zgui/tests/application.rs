//! A whole application, assembled the way an application assembles one, driven with no window.
//!
//! What this is really asserting is that the umbrella's wiring is *connected*. An application
//! written against this crate names a view and a style sheet and nothing else, which means every
//! other decision — which font engine shapes the text, which rasteriser turns its glyphs into
//! pixels, what answers the cascade's font-metric questions, what draws a window — is taken here.
//! A decision taken and not wired up produces a program that opens, runs its whole frame pipeline
//! and puts nothing on the screen, so the assertion is on what reached the display list: a
//! background, and glyphs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use zgui::platform::{AppHandler, PlatformError, Surface};
use zgui::prelude::*;
use zgui::runtime::AppError;

/// A view with one styled box and one run of text in it.
#[component]
fn Panel() -> impl IntoView {
    let (count, set_count) = signal(0);
    view! {
        column(class = "panel") {
            text(class = "panel__label") {{move || format!("count {}", count.get())}}
            control(class = "panel__button", on:click = move |_| set_count.update(|n| *n += 1)) {
                "+"
            }
        }
    }
}

/// The sheet the window is styled by.
const SHEET: &str = css!(
    ":root { background-color: #101216; color: #ffffff; font-family: sans-serif }
     .panel { padding: 16px; gap: 8px; background-color: #202634; border-radius: 8px }
     .panel__label { font-size: 18px }"
);

/// How many frames the renderer was asked to draw.
static FRAMES: AtomicU64 = AtomicU64::new(0);
/// How many glyph sprites reached the display list, over every frame.
static SPRITES: AtomicUsize = AtomicUsize::new(0);
/// How many quads did.
static QUADS: AtomicUsize = AtomicUsize::new(0);

/// A renderer that draws nowhere and counts what it was handed.
///
/// It stands in for the graphics device and for nothing else: everything above it — the document,
/// the cascade, layout, shaping, rasterisation and the paint stage — is the real thing.
#[derive(Debug, Default)]
struct Counting {
    /// Where the glyph tiles go, so that the upload path runs for real.
    atlas: zgui::atlas::MemorySink,
    /// What it was pointed at.
    target: Option<zgui::render::RenderTarget>,
}

impl zgui::render::Renderer for Counting {
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
        scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        use zgui::scene::PrimitiveKind;
        FRAMES.fetch_add(1, Ordering::Relaxed);
        for op in scene.ops() {
            match op.kind {
                PrimitiveKind::MonoSprite
                | PrimitiveKind::SubpixelSprite
                | PrimitiveKind::ColorSprite => {
                    SPRITES.fetch_add(1, Ordering::Relaxed);
                }
                PrimitiveKind::Quad => {
                    QUADS.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
        }
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.atlas
    }
}

/// Drives the application over buffers for a few turns and stops.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(4);
    harness.shut_down();
    Ok(())
}

#[test]
fn an_application_built_through_the_umbrella_draws_its_boxes_and_its_glyphs() {
    app()
        .with_title("umbrella")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
            let mut renderer = Counting::default();
            zgui::render::Renderer::configure(&mut renderer, target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
        }))
        .run_on(buffers, || view! { Panel() })
        .expect("the application ran");

    assert!(
        FRAMES.load(Ordering::Relaxed) >= 1,
        "no frame was drawn at all"
    );
    assert!(
        QUADS.load(Ordering::Relaxed) >= 1,
        "the styled boxes reached no display list"
    );
    assert!(
        SPRITES.load(Ordering::Relaxed) >= 5,
        "the text was laid out and no glyph was drawn: {} sprites",
        SPRITES.load(Ordering::Relaxed)
    );
}
