//! What a running animation costs a real application, and whether it actually moves.
//!
//! Every case here mounts a document, styles it with a real stylesheet, and drives the real loop.
//! That matters more here than almost anywhere else: an animation has two halves that fail
//! independently and look identical from inside. It can be advancing correctly while the loop has
//! nothing to wake it for — the values are right, the counters are right, and the screen is frozen
//! mid-fade. Or the loop can be waking correctly while the picture never changes, because the
//! painter replayed the frame it drew first. A test that calls `advance` and `frame` by hand sees
//! neither, so both are asserted on directly, over frames the loop asked for itself.
//!
//! # Why every case here takes the counter lock
//!
//! The counters are process-global and every case in this file animates something, so every case
//! bumps them. A case that *reads* one therefore has to exclude the ones that merely bump — which
//! is all of them, whether they read a counter or not. A lock held only by the readers leaves the
//! writers running beside them, and the failure that produces is a count that is right on its own
//! and wrong when the file is run whole.

mod support;

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_profile::{COUNTERS_ENABLED, counter};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// Held for the whole of any case that reads a counter.
static COUNTERS: Mutex<()> = Mutex::new(());

/// A little more than one frame at the surface's refresh rate.
///
/// The animation deadline is exactly one refresh interval away, so a clock moved by exactly that
/// much lands on it rather than past it and the loop is entitled to keep waiting. Rounding up is
/// what makes each step here one frame and not, occasionally, none.
const FRAME: Duration = Duration::from_millis(17);

/// Takes the counter lock and zeroes every counter.
fn measuring() -> MutexGuard<'static, ()> {
    let guard = COUNTERS.lock().unwrap_or_else(|held| held.into_inner());
    counter::reset();
    guard
}

/// One button, filling a known rectangle, whose background transitions on hover.
const HOVER_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         .btn { display: block; width: 200px; height: 100px;
                                background-color: rgb(16, 16, 16);
                                transition: background-color 400ms linear }
                         .btn:hover { background-color: rgb(240, 240, 240) }";

/// Eight identically styled buttons in a row, each transitioning on hover.
const ROW_CSS: &str = "root { display: block; width: 400px; height: 300px }
                       .btn { display: block; width: 40px; height: 40px;
                              background-color: rgb(16, 16, 16);
                              transition: background-color 400ms linear }
                       .btn:hover { background-color: rgb(240, 240, 240) }";

/// Five hundred pulsing skeletons: the loading state of every component library there is.
const PULSE_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         @keyframes pulse { from { opacity: 1 } to { opacity: 0.4 } }
                         .cell { display: block; width: 4px; height: 4px;
                                 background-color: rgb(200, 200, 200);
                                 animation: pulse 2s linear infinite }";

/// A panel that fades once and is meant to stay faded, which is what `forwards` means.
const FORWARDS_CSS: &str = "root { display: block; width: 400px; height: 300px }
                            @keyframes fade { from { opacity: 1 } to { opacity: 0.2 } }
                            .panel { display: block; width: 100px; height: 100px;
                                     background-color: rgb(200, 200, 200);
                                     animation: fade 200ms linear forwards }";

/// A panel that fades out when a signal says so, so its end can be listened for.
const FADE_CSS: &str = "root { display: block; width: 400px; height: 300px }
                        .panel { display: block; width: 100px; height: 100px;
                                 background-color: rgb(9, 9, 9) }
                        .panel.gone { opacity: 0; transition: opacity 200ms linear }";

/// An application holding one button at the top left of the window.
fn one_button() -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app(HOVER_CSS, |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::column()
            .class("root")
            .child(zgui_elements::column().class("btn"));
        Box::new(view.into_view().build(cx))
    })
}

/// A pointer event at a point inside the first button.
fn pointer_at(action: PointerAction, x: f32, y: f32) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

#[test]
fn button_hover_transition_takes_the_cheap_animation_path() {
    let _guard = measuring();
    let mut harness = one_button();
    harness.settle(8);

    // A real hover, through the real router, so the transition starts the way it starts in an
    // application rather than because a test wrote a class.
    harness.deliver_to_first(pointer_at(PointerAction::Moved, 10.0, 10.0));
    harness.settle(8);

    harness.reset_counts();
    counter::reset();
    harness.advance(FRAME);
    let frames = harness.pump();
    let frame = counter::snapshot();

    assert_eq!(frames, 1, "the reached deadline produced exactly one frame");
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_restyled, 0,
            "the transition went through the cascade"
        );
        assert_eq!(
            frame.tier_b_transitions, 1,
            "the transition did not take the cheap path"
        );
    }
    // The assertion that separates "the transition is running" from "the transition is running and
    // the loop will come back for it". The two above pass either way; this is the one that fails
    // when the animating bit is marked on one path only, and on a real event loop that is a
    // transition which ticks once and stops.
    assert!(
        harness.parked_deadline().is_some(),
        "the loop parked with no deadline: the transition would never tick again"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_transition_advances_frame_over_frame_on_an_idle_loop() {
    let _guard = measuring();
    // Nothing here calls `frame`. Every frame was asked for by the loop itself, because something
    // told it to come back — which is one of the two halves under test.
    //
    // The other half is what reaches the *display list*, and it is asserted on rather than the
    // interpolated value, because the two fail independently. The lowered style a button is drawn
    // through is shared and does not move while a transition runs, so a per-fragment paint cache
    // that does not know about the override replays the transition's first frame for the whole of
    // its length: the values advance, the obligations are marked, the damage is right, and the
    // picture on the screen never changes.
    let fills: Fills = Rc::new(RefCell::new(Vec::new()));
    let mut harness = drawn_into(HOVER_CSS, &fills, |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::column()
            .class("root")
            .child(zgui_elements::column().class("btn"));
        Box::new(view.into_view().build(cx))
    });
    harness.settle(8);
    harness.deliver_to_first(pointer_at(PointerAction::Moved, 10.0, 10.0));
    harness.settle(8);

    let mut drawn = Vec::new();
    for _ in 0..6 {
        assert!(
            harness.parked_deadline().is_some(),
            "the loop stopped asking to be woken while the transition was still running"
        );
        fills.borrow_mut().clear();
        harness.advance(FRAME);
        assert_eq!(harness.pump(), 1, "the deadline produced no frame");
        drawn.push(brightest_fill(&fills));
    }

    let first = drawn[0];
    assert!(
        drawn.iter().any(|value| *value != first),
        "the button was painted in the same colour on all six frames: {drawn:?}"
    );
    assert!(
        drawn.windows(2).all(|pair| pair[1] >= pair[0]),
        "the painted colour did not move towards the transition's destination: {drawn:?}"
    );
    harness.assert_park_invariant();
}

/// The solid fills of every frame drawn by one window, newest last.
type Fills = Rc<RefCell<Vec<Vec<zgui_color::Color>>>>;

/// The brightest solid fill in the last frame, quantised so a comparison is exact.
///
/// The window's only non-transparent fills are the button's background, which is what the
/// transition moves; taking the brightest is what identifies it without the test knowing anything
/// about the order the display list happens to be in.
fn brightest_fill(fills: &Fills) -> u32 {
    fills
        .borrow()
        .last()
        .into_iter()
        .flatten()
        .map(|color| (color.to_premultiplied_srgb()[0] * 10_000.0) as u32)
        .max()
        .unwrap_or_default()
}

/// An application under `css`, with every frame's solid fills recorded.
///
/// The recording renderer is what separates the two halves an animation fails in independently: the
/// override column says what the animation computed, and this says what reached the display list.
fn drawn_into<V>(
    css: &str,
    fills: &Fills,
    view: V,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn zgui_view::Anchor> + 'static,
{
    let factory = Rc::clone(fills);
    let handler = zgui_runtime::App::new()
        .with_title("anim")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn zgui_platform::Surface>, target: RenderTarget| {
                let mut renderer = Fill {
                    fills: Rc::clone(&factory),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, zgui_runtime::AppError>(Box::new(renderer))
            },
        ))
        .into_handler(view)
        .expect("the reactive runtime installs");
    zgui_platform_headless::Harness::new(handler)
}

/// A renderer that records the solid fills of every frame and draws nothing.
struct Fill {
    /// Where the frames go.
    fills: Fills,
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go into plain memory, so the upload path still runs.
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for Fill {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, scene: &zgui_scene::Scene, _damage: &zgui_bits::DamageSet) -> FrameOutcome {
        self.fills.borrow_mut().push(
            scene
                .primitives
                .quads
                .iter()
                .filter_map(|quad| match scene.paints.get(quad.fill.id()?) {
                    Some(zgui_scene::Paint::Solid(color)) => Some(*color),
                    _ => None,
                })
                .collect(),
        );
        FrameOutcome::Presented(zgui_render::FrameStats {
            vector_passes: 0,
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: MemoryReport::ZERO,
        })
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

#[test]
fn animating_one_button_does_not_touch_its_seven_identical_siblings() {
    let _guard = measuring();
    // The sharing bug, in the shape it actually takes. Eight buttons with one style between them
    // share one lowered paint description by construction, so an animated value written into that
    // description fades all eight. The unhover half is the one that bites: the moment the pointer
    // leaves, the element's computed style is the one its siblings have, so it is being drawn
    // through the very same entry they are.
    let fills: Fills = Rc::new(RefCell::new(Vec::new()));
    let mut harness = drawn_into(ROW_CSS, &fills, |cx: &mut BuildCx<'_>| {
        let mut row = zgui_elements::column().class("root");
        for _ in 0..8 {
            row = row.child(zgui_elements::column().class("btn"));
        }
        Box::new(row.into_view().build(cx))
    });
    harness.settle(8);

    // The fourth button, which the layout stacks at y = 120.
    harness.deliver_to_first(pointer_at(PointerAction::Moved, 20.0, 140.0));
    harness.settle(8);
    let others = siblings_of_third(&harness);

    let mut moved_while_hovering = 0;
    for _ in 0..4 {
        fills.borrow_mut().clear();
        harness.advance(FRAME);
        harness.pump();
        assert_eq!(
            siblings_of_third(&harness),
            others,
            "hovering one button moved the values of the others"
        );
        moved_while_hovering += assert_moving_alone(&fills, "hovering");
    }
    assert!(
        moved_while_hovering > 0,
        "no button was ever painted mid-transition, so `at most one was` proves nothing"
    );

    // The killer case. The pointer has left, so the element's computed style is once again the one
    // its siblings have — it is being drawn through the very entry they are, and a value written
    // into that entry fades all eight together.
    harness.deliver_to_first(pointer_at(PointerAction::Moved, 300.0, 280.0));
    harness.settle(8);
    let mut moved_while_unhovering = 0;
    for _ in 0..4 {
        fills.borrow_mut().clear();
        harness.advance(FRAME);
        harness.pump();
        assert_eq!(
            siblings_of_third(&harness),
            others,
            "unhovering one button moved the values of the others"
        );
        moved_while_unhovering += assert_moving_alone(&fills, "unhovering");
    }
    assert!(
        moved_while_unhovering > 0,
        "no button was ever painted mid-transition, so `at most one was` proves nothing"
    );
}

/// Fails if more than one button was painted in a colour the stylesheet never names.
///
/// The stylesheet gives a button exactly two backgrounds. Anything between them is a value only a
/// running transition produces, and exactly one button is running one — so a frame in which two
/// carry such a value is a frame in which the interpolated value reached somewhere shared. This is
/// the half the override column cannot report on: that column is per node by construction, so an
/// assertion made against it is true whatever the painter went on to do with what it read.
///
/// Returns how many frames had a button mid-transition in them, because *nothing painted at all*
/// satisfies "at most one" perfectly. The caller asserts that number is not zero, which is what
/// stops the whole case passing over a transition that never ran and a window that drew nothing.
fn assert_moving_alone(fills: &Fills, phase: &str) -> usize {
    let mut frames_with_one = 0;
    for frame in fills.borrow().iter() {
        let moving = frame
            .iter()
            .map(|color| (color.to_premultiplied_srgb()[0] * 10_000.0) as u32)
            .filter(|value| {
                *value != 0 && !(626..=628).contains(value) && !(9409..=9411).contains(value)
            })
            .count();
        assert!(
            moving <= 1,
            "while {phase}, {moving} buttons were painted in a colour only the transition produces"
        );
        frames_with_one += usize::from(moving == 1);
    }
    frames_with_one
}

#[test]
fn five_hundred_animations_advance_every_frame_without_restyling_any_of_them() {
    let _guard = measuring();
    let mut harness = support::app(PULSE_CSS, |cx: &mut BuildCx<'_>| {
        let mut grid = zgui_elements::column().class("root");
        for _ in 0..500 {
            grid = grid.child(zgui_elements::column().class("cell"));
        }
        Box::new(grid.into_view().build(cx))
    });
    harness.settle(8);
    // The animation is created by the first cascade and starts on the tick that follows it.
    harness.advance(FRAME);
    harness.pump();

    let before = pulsing_opacities(&harness);
    harness.reset_counts();
    counter::reset();
    harness.advance(FRAME);
    assert_eq!(harness.pump(), 1);
    let frame = counter::snapshot();
    let after = pulsing_opacities(&harness);

    // The cheap path having been *taken* five hundred times is not the same claim as five hundred
    // animations having *advanced*, and the counter cannot tell them apart: it is bumped once per
    // element per frame whether the value it wrote moved or not.
    assert_eq!(after.len(), 500, "not every cell carries an override");
    assert!(
        before.len() == 500 && before != after,
        "five hundred cells took the cheap path and none of their values moved"
    );

    if COUNTERS_ENABLED {
        assert_eq!(
            frame.tier_b_transitions, 500,
            "not every pulsing cell took the cheap path"
        );
        assert_eq!(
            frame.elements_restyled, 0,
            "a screen that is waiting restyled itself"
        );
    }
    assert!(harness.parked_deadline().is_some());
    harness.assert_park_invariant();
}

#[test]
fn a_forwards_fill_mode_holds_the_last_keyframe_after_the_animation_has_ended() {
    let _guard = measuring();
    // What `animation-fill-mode: forwards` is *for*: the animation stops, and the value it stopped
    // at stays. The failure it replaces is not subtle on a screen and is invisible from inside the
    // animation — every value it produced was right, every edge it reported was right, and one
    // frame after the last one the panel snaps back to full opacity, because the machinery that
    // holds the value is the animation still being there to hold it.
    //
    // Both halves are asserted, because each passes without the other. Holding the value while
    // still asking for frames is a finished animation that wakes the process sixty times a second
    // for ever; parking without holding it is the snap-back.
    let fills: Fills = Rc::new(RefCell::new(Vec::new()));
    let mut harness = drawn_into(FORWARDS_CSS, &fills, |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::column()
            .class("root")
            .child(zgui_elements::column().class("panel"));
        Box::new(view.into_view().build(cx))
    });
    harness.settle(8);

    let mut drawn = Vec::new();
    for _ in 0..24 {
        fills.borrow_mut().clear();
        harness.advance(FRAME);
        harness.pump();
        if !fills.borrow().is_empty() {
            drawn.push(brightest_fill(&fills));
        }
    }

    // `rgb(200, 200, 200)` at the last keyframe's opacity, premultiplied and quantised the way
    // `brightest_fill` quantises: 200/255 × 0.2. The number is the stylesheet's, not a recording.
    let held = ((200.0 / 255.0) * 0.2 * 10_000.0) as u32;
    let last = *drawn.last().expect("the panel was drawn at least once");
    assert!(
        last.abs_diff(held) <= 1,
        "the panel did not settle at the last keyframe: {drawn:?}"
    );
    assert!(
        drawn.iter().any(|value| *value > held + 100),
        "the panel never faded at all, so settling faded proves nothing: {drawn:?}"
    );
    assert!(
        harness.parked_deadline().is_none(),
        "a finished animation kept asking for frames"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_transition_that_ends_reports_its_end_to_a_listener() {
    let _guard = measuring();
    // What `Presence` is built on: the exit animation's *actual* end, rather than a duration
    // guessed in code that drifts from the one in the stylesheet the first time it is edited.
    let gone = RwSignal::new(false);
    let ended = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let elapsed = std::rc::Rc::new(std::cell::Cell::new(Duration::ZERO));
    let counted = std::rc::Rc::clone(&ended);
    let timed = std::rc::Rc::clone(&elapsed);
    let mut harness = support::app(FADE_CSS, move |cx: &mut BuildCx<'_>| {
        let counted = std::rc::Rc::clone(&counted);
        let timed = std::rc::Rc::clone(&timed);
        let view = zgui_elements::column().class("root").child(
            zgui_elements::column()
                .class("panel")
                .class_toggle(zgui_interned::ClassName::new("gone"), move || gone.get())
                .on(zgui_view::events::TRANSITION_END, move |event| {
                    counted.set(counted.get() + 1);
                    timed.set(event.elapsed);
                }),
        );
        Box::new(view.into_view().build(cx))
    });
    harness.settle(8);

    gone.set(true);
    harness.settle(8);
    assert_eq!(ended.get(), 0, "the transition ended before it began");

    harness.run_for(Duration::from_millis(400), FRAME);
    assert_eq!(
        ended.get(),
        1,
        "the transition finished and nothing was told about it"
    );
    // What CSS says `transitionend` reports: the transition's own duration, which is the number in
    // the stylesheet. A frame arrives when it arrives, so reporting the wall time since the
    // transition started reports the duration plus however late this frame was — a number that
    // looks right, is never the same twice, and disagrees with the stylesheet by a frame.
    let reported = elapsed.get();
    assert!(
        reported.abs_diff(Duration::from_millis(200)) < Duration::from_millis(1),
        "the reported elapsed time was {reported:?}, not the transition's 200ms duration"
    );
    assert!(
        harness.parked_deadline().is_none(),
        "the loop kept a deadline for an animation that had ended"
    );
}

/// Every pulsing cell's currently overridden opacity, quantised so a comparison is exact.
fn pulsing_opacities(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u32> {
    overrides_of(harness, "cell", |over| {
        over.opacity.map(|value| (value * 100_000.0) as u32)
    })
}

/// The overridden background of every button except the fourth.
fn siblings_of_third(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u32> {
    let mut all = buttons(harness);
    if all.len() > 3 {
        all.remove(3);
    }
    all
}

/// Every button's currently overridden background, quantised so a comparison is exact.
fn buttons(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u32> {
    overrides_of(harness, "btn", |over| {
        over.background_color
            .map(|color| (color.raw_components()[0] * 10_000.0) as u32)
    })
}

/// What `read` reports for every element carrying `class`, with a zero for the ones overriding
/// nothing, in document order.
fn overrides_of(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    class: &str,
    read: impl Fn(&zgui_dom::side::AnimOverride) -> Option<u32>,
) -> Vec<u32> {
    let window = harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window");
    let document = window.document().borrow();
    let mut found = Vec::new();
    for index in 0..document.store().slot_count() {
        let index = zgui_dom::NodeIndex::new(index as u32);
        let Some(_) = document.store().try_core(index) else {
            continue;
        };
        let key = document.store().key_of(index);
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| &**held == class)
        {
            continue;
        }
        let over = document
            .store()
            .columns()
            .anim
            .get(key)
            .and_then(Option::as_ref);
        found.push(over.and_then(|held| read(held)).unwrap_or_default());
    }
    found
}
