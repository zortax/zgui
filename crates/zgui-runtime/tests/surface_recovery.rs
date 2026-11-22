//! A surface retry must receive the scene that a full-target repaint needs.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::{Surface, SurfaceEvent};
use zgui_platform_headless::Harness;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    SkipReason, TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::{PrimitiveKind, Scene};
use zgui_view::{BuildCx, IntoView, View};

/// The damage and active primitives handed to the renderer, before it widens damage.
#[derive(Debug)]
struct Drawn {
    full: bool,
    empty: bool,
    quads: usize,
}

/// A failure is armed only after the first visible frame has settled.
#[derive(Default)]
struct Script {
    draw: Cell<Option<FrameOutcome>>,
    present: Cell<Option<FrameOutcome>>,
    frames: RefCell<Vec<Drawn>>,
}

struct Scripted {
    inner: zgui_testkit_scene::CaptureRenderer,
    script: Rc<Script>,
}

impl Renderer for Scripted {
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
        self.script.frames.borrow_mut().push(Drawn {
            full: damage.is_full(),
            empty: damage.is_empty(),
            quads: scene.remap(PrimitiveKind::Quad).len(),
        });
        let drawn = self.inner.draw(scene, damage);
        self.script.draw.take().unwrap_or(drawn)
    }

    fn present_held(&mut self) -> FrameOutcome {
        self.script
            .present
            .take()
            .unwrap_or(FrameOutcome::Skipped(SkipReason::Undamaged))
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

fn window() -> (Harness<Runtime>, Rc<Script>, RwSignal<bool>) {
    let script = Rc::new(Script::default());
    let changed = RwSignal::new(false);
    let handed = Rc::clone(&script);
    let handler = App::new()
        .with_size(400.0, 300.0)
        .with_stylesheet(
            "root { display: block; width: 400px; height: 300px }
             .block { display: block; width: 40px; height: 40px;
                      background-color: #ff0000; margin: 20px }
             .second { margin-left: 200px; background-color: #0000ff }
             .changed { background-color: #00ff00 }",
        )
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Scripted {
                    inner: zgui_testkit_scene::CaptureRenderer::new(),
                    script: Rc::clone(&handed),
                };
                renderer.configure(target);
                Ok::<_, AppError>(Box::new(renderer) as Box<dyn Renderer>)
            },
        ))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::r#box()
                            .class("block")
                            .class_toggle(zgui_view::ClassName::new("changed"), move || {
                                changed.get()
                            }),
                    )
                    .child(zgui_elements::r#box().class("block").class("second"))
                    .into_view()
                    .build(cx),
            ) as Box<dyn zgui_view::Anchor>
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(60_000));
    harness.settle(8);
    assert!(harness.platform().offscreens()[0].is_visible());
    assert!(script.frames.borrow().iter().any(|frame| frame.quads >= 2));
    script.frames.borrow_mut().clear();
    (harness, script, changed)
}

fn retry(outcome: FrameOutcome, held: bool, expect_full: bool) {
    let (mut harness, script, changed) = window();
    let surface = Arc::clone(&harness.platform().offscreens()[0]);
    script.draw.set(Some(if held {
        FrameOutcome::Skipped(SkipReason::Held)
    } else {
        outcome
    }));
    changed.set(true);
    harness.settle(4);
    assert!(script.draw.get().is_none(), "the fault was not reached");
    {
        let frames = script.frames.borrow();
        let frame = frames.first().expect("the colour change must draw");
        assert!(!frame.full && !frame.empty, "{frame:?}");
        assert_eq!(frame.quads, 1, "only the changed block was emitted");
    }
    if held {
        script.present.set(Some(outcome));
        harness.deliver(surface.id(), SurfaceEvent::PresentReady);
        assert!(script.present.get().is_none());
    }
    script.frames.borrow_mut().clear();
    harness.advance(Duration::from_millis(20));
    harness.settle(4);
    {
        let frames = script.frames.borrow();
        let frame = frames.first().expect("the failed surface must be retried");
        if expect_full {
            assert!(frame.full, "{outcome:?}, held={held}: {frame:?}");
            assert!(
                frame.quads >= 2,
                "a full repaint needs both unchanged blocks: {frame:?}"
            );
        } else {
            assert!(
                frame.empty,
                "the composed picture still survives: {frame:?}"
            );
        }
    }
    // Recovery must not turn ordinary idle redraws into permanent full repainting.
    script.frames.borrow_mut().clear();
    surface.request_redraw();
    harness.settle(4);
    let frames = script.frames.borrow();
    assert!(!frames.is_empty());
    assert!(frames.iter().all(|frame| frame.empty), "{frames:?}");
    harness.assert_park_invariant();
}

#[test]
fn an_outdated_draw_reemits_unchanged_content() {
    retry(FrameOutcome::Skipped(SkipReason::Outdated), false, true);
}

#[test]
fn a_recovered_draw_reemits_unchanged_content() {
    retry(FrameOutcome::Recovered, false, true);
}

#[test]
fn an_outdated_held_present_reemits_unchanged_content() {
    retry(FrameOutcome::Skipped(SkipReason::Outdated), true, true);
}

#[test]
fn a_recovered_held_present_reemits_unchanged_content() {
    retry(FrameOutcome::Recovered, true, true);
}

#[test]
fn a_validation_retry_keeps_the_composed_picture() {
    retry(FrameOutcome::Skipped(SkipReason::Validation), false, false);
}
