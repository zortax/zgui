//! Three things a running animation is supposed to report or honour, driven through a real window.
//!
//! Each of these is invisible from inside the engine: the values move correctly whether or not a
//! transition ever announced that it was created, whether or not a paused animation stops, and
//! whether or not an animated width reaches the layout pass. All three are asserted here, against
//! a real sheet and the real loop, because that is the only place the answer is observable.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// A little more than one frame at the surface's refresh rate.
const FRAME: Duration = Duration::from_millis(17);

/// Runs frames until nothing is animating, or for long enough that nothing ever will be.
fn settle_animations(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    for _ in 0..60 {
        harness.advance(FRAME);
        harness.settle(8);
    }
}

// ---- transitionrun --------------------------------------------------------------------------

/// A transition with a delay, so the moment it is created and the moment it starts are apart.
const DELAYED_CSS: &str = "root { display: block; width: 400px; height: 300px }
                           .btn { display: block; width: 200px; height: 100px;
                                  background-color: rgb(16, 16, 16);
                                  transition: background-color 200ms linear 100ms }
                           .btn:hover { background-color: rgb(240, 240, 240) }";

/// A transition announces that it was created before it announces that it began.
///
/// The two are different events for a reason an author can act on: a control that is about to move
/// is already committed to moving, and `transitionrun` is the only moment at which that is known.
/// Reported for a transition and never for an animation, because an animation has no creation event.
#[test]
fn a_transition_says_it_was_created_before_it_says_it_started() {
    let seen: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let record = Rc::clone(&seen);

    let mut harness = support::app(DELAYED_CSS, move |cx: &mut BuildCx<'_>| {
        let on_run = Rc::clone(&record);
        let on_start = Rc::clone(&record);
        let on_end = Rc::clone(&record);
        let view = zgui_elements::r#box()
            .class("btn")
            .on(
                zgui_view::events::TRANSITION_RUN,
                move |_ev: &mut zgui_view::EventCx<'_, zgui_view::events::TransitionRun>| {
                    on_run.borrow_mut().push("run");
                },
            )
            .on(
                zgui_view::events::TRANSITION_START,
                move |_ev: &mut zgui_view::EventCx<'_, zgui_view::events::TransitionStart>| {
                    on_start.borrow_mut().push("start");
                },
            )
            .on(
                zgui_view::events::TRANSITION_END,
                move |_ev: &mut zgui_view::EventCx<'_, zgui_view::events::TransitionEnd>| {
                    on_end.borrow_mut().push("end");
                },
            )
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    harness.deliver_to_first(SurfaceEvent::Pointer {
        event: PointerEvent::mouse(Point::new(CssPx(20.0), CssPx(20.0))),
        action: PointerAction::Moved,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);
    harness.advance(FRAME);
    harness.settle(4);

    assert_eq!(
        seen.borrow().as_slice(),
        ["run"],
        "the delay has not run out, so the transition has been created and has not begun",
    );

    settle_animations(&mut harness);
    assert_eq!(
        seen.borrow().as_slice(),
        ["run", "start", "end"],
        "each moment is announced once, in the order they happen",
    );
}

// ---- animation-play-state -------------------------------------------------------------------

/// An animation that can be paused by a class, over a property the cheap tier interpolates.
const PAUSE_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         @keyframes fade { from { opacity: 1 } to { opacity: 0 } }
                         .fader { display: block; width: 200px; height: 100px;
                                  background-color: rgb(30, 30, 30);
                                  animation: fade 2s linear }
                         .fader.held { animation-play-state: paused }";

/// A paused animation holds its value and lets the loop park.
///
/// Both halves matter and each fails on its own. An animation that went on advancing while paused
/// is a property that does nothing; one that stopped advancing while the loop went on waking at the
/// refresh rate is a window burning a core over a value that is not moving.
#[test]
fn a_paused_animation_holds_its_value_and_lets_the_window_sleep() {
    let held = RwSignal::new(false);
    let mut harness = support::app(PAUSE_CSS, move |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::r#box()
            .class("fader")
            .class_toggle(zgui_interned::ClassName::new("held"), move || held.get())
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    // Let it run for a while, so there is something to hold.
    for _ in 0..8 {
        harness.advance(FRAME);
        harness.settle(4);
    }
    assert!(
        harness.app().windows()[0].is_animating(),
        "nothing is animating, so pausing it would prove nothing",
    );

    held.set(true);
    harness.settle(8);
    harness.advance(FRAME);
    harness.settle(8);

    assert!(
        !harness.app().windows()[0].is_animating(),
        "the animation is paused and the loop is still being woken for it",
    );

    // And it starts again where it stopped rather than at the beginning.
    held.set(false);
    harness.settle(8);
    harness.advance(FRAME);
    harness.settle(8);
    assert!(
        harness.app().windows()[0].is_animating(),
        "unpausing did not start it again",
    );
}

// ---- an animated length ----------------------------------------------------------------------

/// A bar that grows, which is an animation nothing but the cascade can serve.
const GROW_CSS: &str = "root { display: block; width: 400px; height: 300px }
                        @keyframes grow { from { width: 40px } to { width: 240px } }
                        .bar { display: block; height: 20px;
                               background-color: rgb(30, 30, 30);
                               animation: grow 1s linear forwards }
                        .after { display: block; width: 20px; height: 20px }";

/// The border-box width of the first fragment of every element carrying `class`.
fn widths(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    class: &str,
) -> Vec<i32> {
    let window = harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    let mut found = Vec::new();
    for index in 0..document.store().slot_count() {
        let index = zgui_dom::NodeIndex::new(index as u32);
        if document.store().try_core(index).is_none() {
            continue;
        }
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| &**held == class)
        {
            continue;
        }
        let key = document.store().key_of(index);
        for box_key in layout.boxes_of(key) {
            if let Some(&frag) = layout.fragments_of_box(*box_key).first()
                && let Some(fragment) = layout.fragment(frag)
            {
                found.push(fragment.border_box.size.width.0 as i32);
            }
        }
    }
    found
}

/// An animated length is laid out again on every frame, and settles where the last keyframe says.
///
/// A length is the one thing the two cheap tiers cannot serve: a repaint draws the rectangle that
/// already exists and a placement moves it, and neither makes a box a different size. So this is
/// the whole of the cascade tier, driven end to end — and every part of it fails quietly. An
/// animation that never reaches the cascade holds the width it started at while the loop wakes at
/// the refresh rate; one that reaches it and owes no relayout draws a box the layout engine still
/// thinks is forty pixels wide.
#[test]
fn an_animated_length_is_laid_out_again_on_every_frame() {
    let mut harness = support::app(GROW_CSS, move |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::column()
            .child(zgui_elements::r#box().class("bar"))
            .child(zgui_elements::r#box().class("after"))
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    let mut seen = vec![widths(&harness, "bar")[0]];
    for _ in 0..20 {
        harness.advance(FRAME);
        harness.settle(4);
        seen.push(widths(&harness, "bar")[0]);
    }

    assert!(
        seen.windows(2).all(|pair| pair[1] >= pair[0]),
        "the width never goes backwards: {seen:?}",
    );
    assert!(
        seen.last().copied().unwrap_or(0) > seen[0],
        "the width never moved at all: {seen:?}",
    );
    assert!(
        seen.iter().collect::<std::collections::BTreeSet<_>>().len() > 3,
        "the width moved once and then stopped, so it is not being laid out per frame: {seen:?}",
    );

    settle_animations(&mut harness);
    assert_eq!(
        widths(&harness, "bar")[0],
        240,
        "the fill mode holds the last keyframe, so the box stays as wide as it grew",
    );
    assert!(
        !harness.app().windows()[0].is_animating(),
        "the animation has finished and the loop is still being woken for it",
    );
}
