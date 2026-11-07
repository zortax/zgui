//! A frame the platform is not ready for, held by the renderer and presented on the platform's
//! word.
//!
//! The renderer answers [`SkipReason::Held`] when the platform refuses another buffer: the frame is
//! composed and its damage retires, and nothing is presented until
//! [`SurfaceEvent::PresentReady`] arrives. These cases assert the runtime's half of that contract
//! over a real window: a held frame asks for no other frame, the platform's word presents it
//! exactly once, and a window is shown only by a frame that reached its surface.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::{Surface, SurfaceEvent};
use zgui_render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, SkipReason, TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::Scene;
use zgui_view::{BuildCx, IntoView, View};

/// What a test sets and reads about the renderer under its window.
#[derive(Default)]
struct Gate {
    /// Whether the next frames are held.
    hold: Cell<bool>,
    /// How many frames were drawn.
    drawn: Cell<u32>,
    /// Whether a drawn frame waits to be presented.
    held: Cell<bool>,
    /// How many held frames were presented.
    presented: Cell<u32>,
}

/// A capture renderer that holds its frames while the test says so.
struct Holding {
    inner: zgui_testkit_scene::CaptureRenderer,
    gate: Rc<Gate>,
}

impl Renderer for Holding {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        Renderer::configure(&mut self.inner, target);
    }

    fn target(&self) -> Option<RenderTarget> {
        Renderer::target(&self.inner)
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let drawn = self.inner.draw(scene, damage);
        self.gate.drawn.set(self.gate.drawn.get() + 1);
        if self.gate.hold.get() {
            self.gate.held.set(true);
            return FrameOutcome::Skipped(SkipReason::Held);
        }
        drawn
    }

    fn present_held(&mut self) -> FrameOutcome {
        if !self.gate.held.replace(false) {
            return FrameOutcome::Skipped(SkipReason::Undamaged);
        }
        self.gate.presented.set(self.gate.presented.get() + 1);
        FrameOutcome::Presented(FrameStats::default())
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        self.inner.texture_sink()
    }
}

/// A still document over a holding renderer, with nothing settled yet when `hold` is set.
fn window(hold: bool) -> (zgui_platform_headless::Harness<Runtime>, Rc<Gate>) {
    let gate = Rc::new(Gate::default());
    gate.hold.set(hold);
    let handed = Rc::clone(&gate);
    let handler = App::new()
        .with_title("held")
        .with_size(400.0, 300.0)
        .with_stylesheet(
            "root { display: block; width: 400px; height: 300px }
             .block { display: block; width: 100px; height: 100px;
                      background-color: rgb(200, 200, 200) }",
        )
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Holding {
                    inner: zgui_testkit_scene::CaptureRenderer::new(),
                    gate: Rc::clone(&handed),
                };
                Renderer::configure(&mut renderer, target);
                Ok::<_, AppError>(Box::new(renderer) as Box<dyn Renderer>)
            },
        ))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let view = zgui_elements::column()
                .class("root")
                .child(zgui_elements::r#box().class("block"))
                .into_view();
            Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
        })
        .expect("the reactive runtime installs");
    let harness = zgui_platform_headless::Harness::new(handler);
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(60_000));
    (harness, gate)
}

#[test]
fn a_held_frame_asks_for_nothing_until_the_platform_is_ready() {
    let (mut harness, gate) = window(false);
    harness.settle(8);
    gate.hold.set(true);
    let id = harness.platform().offscreens()[0].id();
    harness.platform().offscreens()[0].request_redraw();
    harness.settle(4);
    assert_eq!(gate.drawn.get(), 2, "the held frame was drawn again");
    assert!(gate.held.get());
    assert_eq!(gate.presented.get(), 0);

    harness.deliver(id, SurfaceEvent::PresentReady);
    assert_eq!(gate.presented.get(), 1);
    harness.deliver(id, SurfaceEvent::PresentReady);
    assert_eq!(
        gate.presented.get(),
        1,
        "one held frame was presented twice"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_window_is_shown_by_the_first_frame_that_reaches_its_surface() {
    let (mut harness, gate) = window(true);
    harness.settle(8);
    let surface = Arc::clone(&harness.platform().offscreens()[0]);
    assert!(gate.drawn.get() >= 1, "the first frame never ran");
    assert!(
        !surface.is_visible(),
        "a window was shown before its first frame was presented"
    );

    harness.deliver(surface.id(), SurfaceEvent::PresentReady);
    assert!(surface.is_visible());
}
