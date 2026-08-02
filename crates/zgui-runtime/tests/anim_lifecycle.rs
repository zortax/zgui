//! What a lifecycle handler is told about the animation it is being told about.
//!
//! Everything that keeps content on the screen for its exit animation is written the same way, and
//! there is no other way to write it: change the state the sheet animates on, wait for the end
//! event, and *then* ask whether anything is still running — because a second animation may have
//! started while the first was going, and content unmounted on the first end would cut the rest of
//! it off.
//!
//! That question has to be answered about the moment the handler is standing in. A frame that
//! published the counts only at its end answers it with the number taken *before* the animation the
//! handler is being told about had finished, so the answer is always "still running", the content
//! waits for an end that has already happened, and it is never unmounted at all. Nothing about the
//! animation looks wrong — it runs and it ends — and the surface simply stays, invisible, over the
//! window, swallowing every press.
//!
//! Both tests here drive a real window over the headless platform: a real sheet, a real cascade, a
//! real transition and the real event that ends it.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, GetUntracked, Set};
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// A little more than one frame at the surface's refresh rate.
const FRAME: Duration = Duration::from_millis(17);

/// A box whose background moves when the pointer arrives, so something genuinely runs and ends.
const HOVER_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         .btn { display: block; width: 200px; height: 100px;
                                background-color: rgb(16, 16, 16);
                                transition: background-color 200ms linear }
                         .btn:hover { background-color: rgb(240, 240, 240) }";

/// Runs frames until nothing is animating, or for long enough that nothing ever will be.
fn settle_animations(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    for _ in 0..60 {
        harness.advance(FRAME);
        harness.settle(8);
    }
}

#[test]
fn a_transition_end_handler_is_told_the_transition_that_ended_is_no_longer_running() {
    let held: Rc<Cell<Option<NodeRef>>> = Rc::default();
    let seen: Rc<Cell<Option<usize>>> = Rc::default();
    let recorded = Rc::clone(&held);
    let record_seen = Rc::clone(&seen);

    let mut harness = support::app(HOVER_CSS, move |cx: &mut BuildCx<'_>| {
        let handle = NodeRef::new();
        recorded.set(Some(handle));
        let record_seen = Rc::clone(&record_seen);
        let view = zgui_elements::r#box()
            .class("btn")
            .node_ref(handle)
            .on(
                zgui_view::events::TRANSITION_END,
                move |_ev: &mut zgui_view::EventCx<'_, zgui_view::events::TransitionEnd>| {
                    record_seen.set(Some(handle.running_animations()));
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

    // The control. Without a transition that really started there is no end event to be told about,
    // and the assertion below would pass by never running.
    assert_eq!(
        held.get().expect("the view was built").running_animations(),
        1,
        "no transition started, so nothing here is being tested"
    );

    settle_animations(&mut harness);

    assert_eq!(
        seen.get(),
        Some(0),
        "the handler for the end of the only transition on the element was told it is still \
         running, so anything waiting for that answer waits for ever"
    );
}

// ---- the whole behaviour the number exists for -------------------------------------------------

/// A surface that fades out, and a frame around it that does not move.
const EXIT_CSS: &str = "root { display: block; width: 400px; height: 300px }
                        .surface { display: block; width: 200px; height: 100px;
                                   background-color: rgb(30, 30, 30);
                                   opacity: 1;
                                   transition: opacity 200ms linear }
                        .surface[data-state=\"closed\"] { opacity: 0 }";

/// How many elements in the window carry `class`.
fn count(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>, class: &str) -> usize {
    let window = harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window");
    let document = window.document().borrow();
    let store = document.store();
    let wanted = zgui_interned::ClassName::new(class);
    (0..store.slot_count())
        .map(|slot| zgui_dom::NodeIndex::new(slot as u32))
        .filter(|index| store.try_core(*index).is_some())
        .filter(|index| {
            store
                .classes_of(*index)
                .iter()
                .any(|held| held.as_ref() == wanted.as_str())
        })
        .count()
}

#[test]
fn content_kept_mounted_for_its_exit_transition_is_unmounted_when_that_transition_ends() {
    // The pattern every floating surface in a component library is built out of, written the only
    // way it can be written: nothing here guesses a duration, and the unmount is decided by the end
    // event plus the answer to "is anything still running on me?".
    let mounted = RwSignal::new(true);
    let leaving = RwSignal::new(false);

    let mut harness = support::app(EXIT_CSS, move |cx: &mut BuildCx<'_>| {
        let view = zgui_view::Show::new(
            move || mounted.get(),
            move || {
                let handle = NodeRef::new();
                zgui_view::AnyView::new(
                    zgui_elements::r#box()
                        .class("surface")
                        .node_ref(handle)
                        .attribute(zgui_view::AttrName::new("data-state"), move || {
                            Some(if leaving.get() { "closed" } else { "open" }.to_owned())
                        })
                        .on(
                            zgui_view::events::TRANSITION_END,
                            move |_ev: &mut zgui_view::EventCx<
                                '_,
                                zgui_view::events::TransitionEnd,
                            >| {
                                if leaving.get_untracked() && handle.running_animations() == 0 {
                                    mounted.set(false);
                                }
                            },
                        ),
                )
            },
        )
        .fallback(|| zgui_view::AnyView::new(()))
        .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);
    assert_eq!(
        count(&harness, "surface"),
        1,
        "the surface is on the screen"
    );

    leaving.set(true);
    harness.settle(8);
    harness.advance(FRAME);
    harness.settle(8);
    assert_eq!(
        count(&harness, "surface"),
        1,
        "the surface left before a single frame of its exit was drawn"
    );

    settle_animations(&mut harness);
    assert_eq!(
        count(&harness, "surface"),
        0,
        "the exit transition ended and the surface was never taken away, so it is still over the \
         window with nothing visible on it"
    );
}
