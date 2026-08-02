//! What an application costs before it has drawn anything.
//!
//! The assertion is a count rather than a duration: enumerating the faces installed on a machine
//! is the largest single thing a launch does before it has a window, and an application that ships
//! its own faces has said it wants none of it. A builder that had already enumerated them by the
//! time it was told so would be doing that work for nothing, and no timing test on one machine
//! would ever say which of the two happened.

use std::sync::Arc;

use zgui::app::Fonts;
use zgui::app::fonts::system_collections_built;
use zgui::platform::{AppHandler, PlatformError, Surface};
use zgui::prelude::*;
use zgui::runtime::AppError;

/// A renderer that draws nowhere, so that what is measured is the application and not a driver.
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

/// Drives the application over buffers for a few turns and stops.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(4);
    harness.shut_down();
    Ok(())
}

/// Runs a whole application, with `fonts` if one was named.
fn run(fonts: Option<Fonts>) {
    let mut application = app().with_title("startup").with_size(320.0, 200.0);
    if let Some(fonts) = fonts {
        application = application.with_fonts(fonts);
    }
    application
        .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
            let mut renderer = Nowhere::default();
            zgui::render::Renderer::configure(&mut renderer, target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
        }))
        .run_on(buffers, || view! { column { text {"x"} } })
        .expect("the application ran");
}

/// Both halves run in one test because the count is per process and the two would otherwise race.
#[test]
fn the_font_collection_is_not_built_when_the_caller_supplies_one() {
    assert_eq!(
        system_collections_built(),
        0,
        "no application has been built yet"
    );

    run(Some(Fonts::shipped_only()));
    assert_eq!(
        system_collections_built(),
        0,
        "an application that ships its own faces never enumerates the machine's"
    );

    // And the complement, without which the assertion above would be met by an application that
    // never draws text at all: one that names no faces does get this machine's, exactly once.
    run(None);
    assert_eq!(
        system_collections_built(),
        1,
        "an application that names no faces enumerates them, once"
    );
}
