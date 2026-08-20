//! The starvation latch, driven through a real window with scripted acquisition outcomes.
//!
//! With the compositor notification unwired, the one brake on presenting into a surface the
//! compositor stopped drawing is this latch: a timed-out acquisition parks the window, a probe
//! comes back at a growing distance, and evidence of visibility ends the hold at once. Each of
//! those is asserted here over the loop, because each fails invisibly — a latch that never
//! releases is a frozen window, and one that never engages is an input thread blocking a second
//! per frame behind a workspace switch.

mod support;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::{Surface, SurfaceEvent};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    SkipReason, TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime, RuntimeHost};
use zgui_scene::Scene;
use zgui_view::{BuildCx, IntoView, View, ViewHost};

/// What the next acquisitions are scripted to do; empty means the capture default, which presents.
type Script = Rc<RefCell<VecDeque<FrameOutcome>>>;

/// A capture renderer whose draw outcomes a test scripts.
struct Scripted {
    inner: zgui_testkit_scene::CaptureRenderer,
    script: Script,
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
        let drawn = self.inner.draw(scene, damage);
        self.script.borrow_mut().pop_front().unwrap_or(drawn)
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

/// A still document over a scripted renderer, with a heartbeat driving continuous frames.
fn window_with_script() -> (zgui_platform_headless::Harness<Runtime>, Script) {
    let script: Script = Rc::default();
    let handed = Rc::clone(&script);
    let handler = App::new()
        .with_title("starve")
        .with_size(400.0, 300.0)
        .with_stylesheet(
            "root { display: block; width: 400px; height: 300px }
             .block { display: block; width: 100px; height: 100px;
                      background-color: rgb(200, 200, 200) }",
        )
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Scripted {
                    inner: zgui_testkit_scene::CaptureRenderer::new(),
                    script: Rc::clone(&handed),
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
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(60_000));
    harness.settle(8);
    (harness, script)
}

/// A frame callback that always registers its successor, and how many times it ran.
///
/// The heartbeat is what makes the window continuously animated, so the latch has something
/// visible to park: with it running, a parked cadence is observable as the runs stopping.
fn heartbeat(host: Rc<RuntimeHost>) -> Rc<Cell<u64>> {
    let runs = Rc::new(Cell::new(0u64));
    struct Beat {
        host: Rc<RuntimeHost>,
        runs: Rc<Cell<u64>>,
    }
    fn arm(beat: &Rc<Beat>) {
        let again = Rc::clone(beat);
        beat.host.request_frame_callback(Rc::new(move |_| {
            again.runs.set(again.runs.get() + 1);
            arm(&again);
        }));
    }
    arm(&Rc::new(Beat {
        host,
        runs: Rc::clone(&runs),
    }));
    runs
}

/// One frame of the sixty-hertz output every case here runs on.
const FRAME: Duration = Duration::from_micros(16_667);

/// A pointer sample, which is the commonest evidence of visibility.
fn pointer() -> SurfaceEvent {
    SurfaceEvent::Pointer {
        event: zgui_vocab::PointerEvent::mouse(zgui_geom::Point::new(
            zgui_geom::CssPx(20.0),
            zgui_geom::CssPx(20.0),
        )),
        action: zgui_vocab::PointerAction::Moved,
        modifiers: zgui_vocab::Modifiers::NONE,
        timestamp: zgui_vocab::Timestamp::ORIGIN,
    }
}

#[test]
fn a_timed_out_acquisition_latches_the_window_and_parks_its_animation() {
    let (mut harness, script) = window_with_script();
    let runs = heartbeat(Rc::clone(harness.app().windows()[0].host()));
    harness.settle(4);
    assert!(runs.get() >= 1, "the heartbeat never started");

    script
        .borrow_mut()
        .push_back(FrameOutcome::Skipped(SkipReason::Timeout));
    harness.advance(FRAME);
    harness.pump();
    assert!(
        harness.app().windows()[0].is_starved(),
        "a timed-out acquisition did not latch the window"
    );

    // Under the latch the heartbeat is parked: most of a second of virtual time buys no runs,
    // where an unlatched window would have run it sixty times.
    let before = runs.get();
    let frames = harness.run_for(Duration::from_millis(900), Duration::from_millis(16));
    assert!(
        runs.get() <= before + 1,
        "a starved window kept running its heartbeat: {} more runs",
        runs.get() - before
    );
    assert_eq!(
        frames, 0,
        "a starved window kept presenting: {frames} frames"
    );
    harness.assert_park_invariant();
}

#[test]
fn the_probe_widens_its_wait_while_the_surface_stays_starved() {
    let (mut harness, script) = window_with_script();
    let _runs = heartbeat(Rc::clone(harness.app().windows()[0].host()));
    harness.settle(4);

    // Every acquisition times out from here: the first latches, and each probe re-latches.
    for _ in 0..8 {
        script
            .borrow_mut()
            .push_back(FrameOutcome::Skipped(SkipReason::Timeout));
    }
    harness.advance(FRAME);
    harness.pump();
    assert!(harness.app().windows()[0].is_starved());

    // The probes fire at one, two, four, eight and sixteen seconds, and no more often: fifteen
    // seconds of virtual time holds the first four (1 + 2 + 4 + 8 = 15), each of which consumes
    // one scripted timeout.
    let scripted = script.borrow().len();
    harness.run_for(Duration::from_secs(15), Duration::from_millis(50));
    let consumed = scripted - script.borrow().len();
    assert!(
        (3..=4).contains(&consumed),
        "fifteen seconds held {consumed} probes where the backoff owes about four"
    );
    assert!(
        harness.app().windows()[0].is_starved(),
        "a probe that timed out released the latch"
    );
    harness.assert_park_invariant();
}

#[test]
fn input_ends_the_hold_and_the_heartbeat_resumes_at_once() {
    let (mut harness, script) = window_with_script();
    let runs = heartbeat(Rc::clone(harness.app().windows()[0].host()));
    harness.settle(4);

    script
        .borrow_mut()
        .push_back(FrameOutcome::Skipped(SkipReason::Timeout));
    harness.advance(FRAME);
    harness.pump();
    assert!(harness.app().windows()[0].is_starved());

    harness.deliver_to_first(pointer());
    harness.settle(4);
    assert!(
        !harness.app().windows()[0].is_starved(),
        "input is evidence of visibility and must end the hold"
    );

    // And the heartbeat is paced again: a second of virtual time runs it about sixty times.
    let before = runs.get();
    harness.run_for(Duration::from_secs(1), Duration::from_millis(8));
    assert!(
        runs.get() >= before + 50,
        "the heartbeat stayed parked after the hold ended: {} runs in a second",
        runs.get() - before
    );
    harness.assert_park_invariant();
}

#[test]
fn a_probe_that_presents_releases_the_latch_without_any_event() {
    // The visible-window hiccup: the compositor stalled for a moment, nothing was hidden, and no
    // event will ever say it recovered. The probe is the only way back.
    let (mut harness, script) = window_with_script();
    let runs = heartbeat(Rc::clone(harness.app().windows()[0].host()));
    harness.settle(4);

    script
        .borrow_mut()
        .push_back(FrameOutcome::Skipped(SkipReason::Timeout));
    harness.advance(FRAME);
    harness.pump();
    assert!(harness.app().windows()[0].is_starved());

    // Nothing else is scripted, so the probe's own draw presents.
    let before = runs.get();
    harness.run_for(Duration::from_millis(1_500), Duration::from_millis(16));
    assert!(
        !harness.app().windows()[0].is_starved(),
        "the probe presented and the latch is still holding"
    );
    assert!(
        runs.get() > before + 10,
        "the heartbeat did not resume after the probe recovered"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_timer_under_the_latch_runs_without_presenting_and_keeps_the_probe() {
    let (mut harness, script) = window_with_script();
    harness.settle(4);

    script
        .borrow_mut()
        .push_back(FrameOutcome::Skipped(SkipReason::Timeout));
    // A frame to consume the timeout: the still document owes none on its own, so ask through a
    // zero-delay timer.
    let host = Rc::clone(harness.app().windows()[0].host());
    let poked = Rc::new(Cell::new(0u64));
    let counter = Rc::clone(&poked);
    host.schedule(
        Duration::ZERO,
        zgui_view::Repeat::Once,
        Rc::new(move || counter.set(counter.get() + 1)),
    );
    harness.settle(4);
    assert!(harness.app().windows()[0].is_starved());

    // A timer due long before the probe: its frame runs the callback and skips the present.
    let fired = Rc::new(Cell::new(0u64));
    let counter = Rc::clone(&fired);
    host.schedule(
        Duration::from_millis(200),
        zgui_view::Repeat::Once,
        Rc::new(move || counter.set(counter.get() + 1)),
    );
    harness.run_for(Duration::from_millis(400), Duration::from_millis(50));
    assert_eq!(
        fired.get(),
        1,
        "a timer behind a starved surface must still fire"
    );
    assert!(
        harness.app().windows()[0].is_starved(),
        "a timer's frame must keep the latch: its present would block the loop"
    );
    harness.assert_park_invariant();
}
