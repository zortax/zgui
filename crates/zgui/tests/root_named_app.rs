//! An application whose root component is called `App`, which is what everybody calls it.
//!
//! `App` is the obvious name for a root component, and the idiom beside a component is a `style!`
//! block of the same name — which declares a *type*. A locally declared item shadows a
//! glob-imported one, so a prelude that exported a type called `App` would be silently taken over
//! by the first application that named its root the obvious thing, and the entry point would stop
//! resolving in a program that never mentioned it. The failure is at the framework's front door and
//! reads `no associated function or constant named 'new' found for struct 'App'`.
//!
//! It cannot happen through the name the prelude does export: `#[component]` refuses a name that
//! does not begin with a capital, so nothing a component or its style sheet declares can be spelled
//! `app`. This file is that claim, compiled — every one of the three `App`s below is declared, and
//! the application is still built and driven through the prelude's own entry point.
//!
//! The run-time assertions exist so that this is not a file that merely compiles: an entry point
//! that resolved and then built nothing would leave a compile-only case green.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use zgui::platform::{AppHandler, PlatformError, Surface};
use zgui::prelude::*;
use zgui::runtime::AppError;

// The scoped style sheet a component carries, named after the component. This is what declares a
// *type* called `App` in this module.
style! { App =>
    ":scope { display: flex; padding: 8px }"
}

/// The root component, named the obvious thing.
///
/// Its handler is bound to a name before it is attached, which is the other spelling an application
/// reaches for and which needs [`handler`](zgui::view::handler) to carry a usable type. It is
/// written here rather than in a case of its own because the umbrella's prelude is what has to
/// export it: a constructor reachable only at its own path fixes nothing for an application that
/// imports the prelude and nothing else.
#[component]
fn App() -> impl IntoView {
    let (count, set_count) = signal(0);
    let bump = handler(events::CLICK, move |_| set_count.update(|n| *n += 1));
    view! {
        column(class = App::CLASS) {
            text {{move || count.get().to_string()}}
            control(on:click = bump) {"+"}
        }
    }
}

/// A renderer that counts frames and draws nothing.
#[derive(Default)]
struct Counting {
    /// The surface it was pointed at.
    target: Option<zgui::render::RenderTarget>,
    /// How many quads the last frame put in the display list.
    quads: Rc<Cell<usize>>,
    /// Where its tiles go.
    atlas: zgui::atlas::MemorySink,
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

    fn draw(
        &mut self,
        scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        self.quads
            .set(self.quads.get() + scene.primitives.quads.len());
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats {
            vector_passes: 0,
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: zgui::render::MemoryReport::ZERO,
        })
    }

    fn register_external(
        &mut self,
        _texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        zgui::render::TextureHandle(0)
    }

    fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.atlas
    }
}

#[test]
fn a_root_component_called_app_does_not_take_over_the_entry_point() -> Result<(), AppError> {
    // The style sheet's `App` is a type in this module, and the component's `App` is a function in
    // it. Reading both is what makes the shadowing real rather than hypothetical: if the prelude
    // exported the application type under this name, these two lines would be reading it.
    assert!(App::CLASS.starts_with("zs-"));
    assert!(App::CSS.contains(App::CLASS));

    let quads = Rc::new(Cell::new(0));
    let counted = Rc::clone(&quads);
    let handler = app()
        .with_title("root")
        .with_application_id("dev.zgui.RootNamedApp")
        .with_size(200.0, 120.0)
        .with_stylesheet(":root { background-color: #101216; display: block }")
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: zgui::render::RenderTarget| {
                let mut renderer = Counting {
                    quads: Rc::clone(&counted),
                    ..Counting::default()
                };
                zgui::render::Renderer::configure(&mut renderer, target);
                Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
            },
        ))
        .into_handler(|| view! { App() })?;

    drive(handler)?;
    assert!(
        quads.get() > 0,
        "the application named its root `App` and drew nothing, so the entry point resolved to \
         something that never ran the view"
    );
    Ok(())
}

/// Drives the application over buffers until it has drawn.
fn drive(handler: zgui::app::Handler) -> Result<(), AppError> {
    handler.drive(|app: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = zgui_platform_headless::Harness::new(app);
        harness.settle(4);
        harness.shut_down();
        Ok(())
    })
}
