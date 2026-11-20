//! A surface that loses its hidden frame receives a full frame after it is shown.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::{Surface, SurfaceEvent};
use zgui_render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, SkipReason, TextureHandle,
};
use zgui_runtime::{App, AppError};
use zgui_scene::{PrimitiveKind, Scene};
use zgui_view::{BuildCx, IntoView, View};

#[derive(Debug)]
struct Drawn {
    visible: bool,
    full: bool,
    empty: bool,
    quads: usize,
}

#[derive(Default)]
struct Log {
    frames: RefCell<Vec<Drawn>>,
    surface: RefCell<Option<Arc<zgui_platform_headless::OffscreenSurface>>>,
    hold: Cell<bool>,
    held: Cell<bool>,
}

struct Recording {
    inner: zgui_testkit_scene::CaptureRenderer,
    log: Rc<Log>,
}

impl Renderer for Recording {
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
        self.log.frames.borrow_mut().push(Drawn {
            visible: self
                .log
                .surface
                .borrow()
                .as_ref()
                .expect("the surface was recorded before drawing")
                .is_visible(),
            full: damage.is_full(),
            empty: damage.is_empty(),
            quads: scene.remap(PrimitiveKind::Quad).len(),
        });
        let outcome = self.inner.draw(scene, damage);
        if self.log.hold.replace(false) {
            self.log.held.set(true);
            FrameOutcome::Skipped(SkipReason::Held)
        } else {
            outcome
        }
    }

    fn present_held(&mut self) -> FrameOutcome {
        assert!(self.log.held.replace(false));
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

fn first_visible_frame(survives: bool, held: bool) {
    let log = Rc::new(Log::default());
    log.hold.set(held);
    let factory_log = Rc::clone(&log);
    let handler = App::new()
        .with_size(400.0, 300.0)
        .with_stylesheet(
            "root { display: block; width: 400px; height: 300px }
             .block { display: block; width: 40px; height: 40px;
                      margin: 20px; background-color: red }",
        )
        .with_renderer(Box::new(move |_surface: &Arc<dyn Surface>, target| {
            let mut renderer = Recording {
                inner: zgui_testkit_scene::CaptureRenderer::new(),
                log: Rc::clone(&factory_log),
            };
            renderer.configure(target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn Renderer>)
        }))
        .into_handler(|cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .child(zgui_elements::r#box().class("block"))
                    .child(zgui_elements::r#box().class("block"))
                    .into_view()
                    .build(cx),
            ) as Box<dyn zgui_view::Anchor>
        })
        .expect("the runtime installs");
    let mut harness = zgui_platform_headless::Harness::new(handler);
    let surface = Arc::clone(&harness.platform().offscreens()[0]);
    *log.surface.borrow_mut() = Some(Arc::clone(&surface));
    if !survives {
        surface.set_contents_survive_show(false);
    }
    harness.settle(8);
    if held {
        assert!(!surface.is_visible());
        assert_eq!(log.frames.borrow().len(), 1);
        harness.deliver(surface.id(), SurfaceEvent::PresentReady);
        harness.settle(8);
    }
    assert!(surface.is_visible());
    {
        let frames = log.frames.borrow();
        assert_eq!(frames.len(), if survives { 1 } else { 2 }, "{frames:?}");
        assert!(!frames[0].visible);
        assert!(frames[0].full && frames[0].quads >= 2);
        if !survives {
            let shown = &frames[1];
            assert!(shown.visible && shown.full && shown.quads >= 2, "{shown:?}");
        }
    }
    log.frames.borrow_mut().clear();
    surface.request_redraw();
    harness.settle(8);
    let frames = log.frames.borrow();
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert!(
        frames[0].empty,
        "mapping must invalidate only once: {frames:?}"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_discarded_hidden_frame_is_drawn_again_after_showing() {
    first_visible_frame(false, false);
}

#[test]
fn a_discarded_held_frame_is_drawn_again_after_showing() {
    first_visible_frame(false, true);
}

#[test]
fn a_preserved_hidden_frame_needs_no_extra_draw() {
    first_visible_frame(true, false);
}

#[test]
fn a_preserved_held_frame_needs_no_extra_draw() {
    first_visible_frame(true, true);
}
