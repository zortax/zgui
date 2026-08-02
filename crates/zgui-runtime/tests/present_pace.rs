//! When a frame that has been asked for is allowed to start, over the loop rather than the servo.
//!
//! A loop that draws the instant it is asked composes the world as it was when the last buffer was
//! released and then waits, on the thread that reads input, for the display to release the next
//! one. The frame is therefore a refresh interval old before anybody sees it, and everything that
//! arrived while it waited is answered by the frame after it.
//!
//! Holding the frame back costs nothing and buys the difference — provided three things hold, and
//! all three are over the loop rather than inside the arithmetic:
//!
//! * a window presenting to something that never makes it wait is **never** held, which is what
//!   keeps every other test in this crate, and every offscreen window anywhere, unchanged;
//! * a held frame is **let go**, by a deadline the park installs, rather than waiting for the next
//!   thing that happens to ask for a frame;
//! * the input that arrives during the hold is **queued and drawn by the frame that runs**, which
//!   is the whole of what the hold is for. A hold that dropped it would be a dropped frame.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use zgui_bits::DamageSet;
use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, TextureHandle,
};
use zgui_scene::Scene;
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{BuildCx, IntoView, View};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 400px; height: 300px;
                            background-color: rgb(20, 20, 20) }";

/// What a window presenting to a display with one image spare is made to wait.
const BLOCKS: Duration = Duration::from_millis(10);

/// How many frames a renderer has drawn, shared with the test that installed it.
type Frames = Rc<Cell<u64>>;

/// A renderer that reports having been made to wait, and counts what it drew.
///
/// The wait is the only thing about it that matters. It is what a swap chain with every image
/// spoken for does to the frame that asks for the next one, and it is the sole observation the
/// hold is derived from.
struct Waiting {
    /// How long each frame is reported to have waited for a surface to present into.
    blocks: Duration,
    /// How many frames have been drawn.
    frames: Frames,
    /// The surface being drawn for.
    target: Option<RenderTarget>,
    /// Somewhere for rasterised content to go.
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for Waiting {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, _scene: &Scene, _damage: &DamageSet) -> FrameOutcome {
        self.frames.set(self.frames.get() + 1);
        FrameOutcome::Presented(FrameStats {
            vector_passes: 0,
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: MemoryReport::ZERO,
        })
    }

    fn acquire_block(&self) -> Duration {
        self.blocks
    }

    fn register_external(&mut self, _texture: ExternalTexture) -> TextureHandle {
        TextureHandle(0)
    }

    fn release_external(&mut self, _handle: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }
}

/// Where each pointer sample the document dispatched is recorded.
type Moves = Rc<std::cell::RefCell<Vec<f32>>>;

/// A window whose renderer reports having waited `blocks` for every surface it presented into.
///
/// Every pointer sample the document dispatches is recorded into `moves`, which is how a test asks
/// what the frame that ran was built from rather than only whether one ran.
fn window_waiting(
    blocks: Duration,
    frames: &Frames,
    moves: &Moves,
) -> zgui_platform_headless::Harness<Runtime> {
    let counted = Rc::clone(frames);
    let seen = Rc::clone(moves);
    let handler = App::new()
        .with_title("present-pace")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Waiting {
                    blocks,
                    frames: Rc::clone(&counted),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let seen = Rc::clone(&seen);
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .on(zgui_view::events::POINTER_MOVE, move |event| {
                        seen.borrow_mut().push(event.position.x.0);
                    })
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(8);
    harness
}

/// A pointer moving to `x`, which is input and therefore asks for a frame.
fn pointer_at(x: f32) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::<CssPx, Css>::new(CssPx(x), CssPx(40.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// How long the window is holding the next frame it is asked for.
fn hold(harness: &zgui_platform_headless::Harness<Runtime>) -> Duration {
    harness.app().windows()[0].present_hold()
}

#[test]
fn a_window_that_is_never_made_to_wait_is_never_held() {
    // The guard on everything else. An offscreen surface, and a swap chain with an image to
    // spare, hand a frame a surface the moment it asks — so there is no slack to schedule into and
    // the loop has to be exactly the loop it would be with none of this here. Every other test in
    // this crate is written over a window in this state.
    let frames = Frames::default();
    let moves = Moves::default();
    let mut harness = window_waiting(Duration::ZERO, &frames, &moves);
    let drawn = frames.get();

    harness.deliver_to_first(pointer_at(10.0));
    harness.settle(4);

    assert_eq!(
        hold(&harness),
        Duration::ZERO,
        "a hold was built out of nothing"
    );
    assert_eq!(
        harness.app().windows()[0].held_frames(),
        0,
        "a window with nothing to schedule into held a frame anyway"
    );
    assert!(
        frames.get() > drawn,
        "the frame the pointer asked for never ran"
    );
}

#[test]
fn a_frame_is_held_back_and_then_let_go_by_the_deadline() {
    // The mechanism end to end. The frame is not refused and nothing about it is dropped: it is
    // started later inside the same interval, and what lets it go is a deadline the park installs
    // rather than the next thing that happens to want a frame.
    let frames = Frames::default();
    let moves = Moves::default();
    let mut harness = window_waiting(BLOCKS, &frames, &moves);
    assert!(
        hold(&harness) > Duration::ZERO,
        "a window made to wait ten milliseconds a frame built no hold"
    );

    let drawn = frames.get();
    let before = harness.now();
    harness.deliver_to_first(pointer_at(10.0));
    harness.settle(4);

    assert_eq!(frames.get(), drawn, "the frame was not held back at all");
    assert_eq!(
        harness.app().windows()[0].held_frames(),
        1,
        "the offered frame was not recorded as held"
    );
    let owed = harness
        .parked_deadline()
        .expect("a held frame owes a moment to be let go at");
    assert_eq!(
        owed,
        before + hold(&harness),
        "the loop parked on something other than the moment the frame was owed at"
    );

    harness.advance(owed - before);
    harness.settle(4);
    assert_eq!(
        frames.get(),
        drawn + 1,
        "the deadline came and went without the held frame running"
    );
}

#[test]
fn everything_that_arrives_during_the_hold_is_drawn_by_the_frame_that_runs() {
    // What separates a hold from a dropped frame, and the second half of what it buys: one frame
    // answering a whole burst, composed as late and therefore as recently as it can be. A pointer
    // stream is the case — a hold that restarted on each sample would recede for as long as the
    // pointer moved, and one that dropped them would lose the positions.
    let frames = Frames::default();
    let moves = Moves::default();
    let mut harness = window_waiting(BLOCKS, &frames, &moves);
    let drawn = frames.get();
    let start = harness.now();
    let held = hold(&harness);
    moves.borrow_mut().clear();

    let sent: Vec<f32> = (0..8_i16)
        .map(|step| 10.0 + f32::from(step) * 10.0)
        .collect();
    for x in &sent {
        harness.deliver_to_first(pointer_at(*x));
        harness.settle(4);
        assert_eq!(
            frames.get(),
            drawn,
            "a sample inside the hold ran a frame of its own"
        );
        assert_eq!(
            harness.parked_deadline(),
            Some(start + held),
            "a sample inside the hold pushed the moment the frame was owed at"
        );
        assert!(
            moves.borrow().is_empty(),
            "a sample was dispatched before the frame that was held ran"
        );
    }

    harness.advance(held);
    harness.settle(4);
    assert_eq!(
        frames.get(),
        drawn + 1,
        "eight samples inside one hold did not become exactly one frame"
    );
    assert_eq!(
        *moves.borrow(),
        sent,
        "the one frame that ran was not built from every sample that arrived during the hold"
    );
}
