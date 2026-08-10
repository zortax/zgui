//! Presence driven through real frames, over the virtual clock.

mod harness;

use core::time::Duration;

use harness::Harness;
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::vocab::{AnimationEvent, AnimationPhase, EventKind, Payload};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

/// The surface a presence keeps mounted, carrying the state a style sheet selects on.
#[component]
fn Surface(
    /// The element whose exit animation decides when the content leaves.
    element_ref: NodeRef,
) -> impl IntoView {
    let presence = use_presence();
    view! {
        box(
            class = "popover",
            node_ref = element_ref,
            attr:data-state = move || presence.map(|presence| presence.state_name().to_owned())
        ) {
            "contents"
        }
    }
}

/// A presence over a surface, both driven by the test.
#[component]
fn Wrapped(
    /// Whether the content should be there.
    present: Signal<bool, zgui::reactive::LocalStorage>,
    /// The surface's element.
    surface: NodeRef,
) -> impl IntoView {
    view! {
        Presence(present = present, surface = surface) {
            Surface(element_ref = surface)
        }
    }
}

/// Mounts a presence that starts open.
fn opened() -> (
    Harness,
    NodeRef,
    RwSignal<bool, zgui::reactive::LocalStorage>,
) {
    let harness = Harness::open();
    let present = harness.window.scope.with(|| RwSignal::new_local(true));
    let surface = harness.window.scope.with(NodeRef::new);
    harness.mount(move || {
        view! { Wrapped(present = Signal::from(present), surface = surface) }
    });
    (harness, surface, present)
}

/// Whether the surface is still in the tree.
fn mounted(harness: &Harness, surface: NodeRef) -> bool {
    let Some(node) = surface.get_untracked() else {
        return false;
    };
    use zgui::view::Dom;
    harness.window.dom.parent(node).is_some()
}

/// What the surface's `data-state` says now.
fn state(harness: &Harness, surface: NodeRef) -> Option<String> {
    let node = surface.get_untracked()?;
    harness
        .window
        .dom
        .tree()
        .attribute(node, zgui::view::AttrName::new("data-state"))
}

#[test]
fn an_open_presence_mounts_its_content_and_says_so() {
    let (harness, surface, _present) = opened();
    assert!(mounted(&harness, surface));
    assert_eq!(state(&harness, surface).as_deref(), Some("open"));
}

#[test]
fn with_no_animation_the_content_goes_on_the_frame_after_it_closed() {
    // The `running_animations() == 0` branch. It is deferred by exactly one frame, because the
    // attribute that would have started an animation has not been cascaded yet when the state
    // changes — asking in the same frame would always answer "nothing running" and cut every exit.
    let (harness, surface, present) = opened();
    let node = surface.get_untracked().expect("bound");

    present.set(false);
    harness.window.frame();
    assert_eq!(
        state(&harness, surface).as_deref(),
        Some("closed"),
        "the state is written straight away, so the cascade can act on it"
    );
    assert!(mounted(&harness, surface), "and the content is still there");

    // The next frame, with nothing running.
    harness.window.host.set_running_animations(node, 0);
    harness.window.advance(Duration::ZERO);
    assert!(!mounted(&harness, surface));
}

#[test]
fn with_an_animation_running_the_content_waits_for_it_to_end() {
    let (harness, surface, present) = opened();
    let node = surface.get_untracked().expect("bound");

    present.set(false);
    harness.window.frame();
    // The cascade started an exit animation, which is what the engine would now report.
    harness.window.host.set_running_animations(node, 1);
    harness.window.advance(Duration::ZERO);

    assert!(
        mounted(&harness, surface),
        "the content left before its animation had a chance to run"
    );

    // Time passes — as much as any exit this library writes takes, and several times over —
    // and nothing unmounts on its own, because nothing here guesses a duration.
    harness.window.advance(Duration::from_millis(500));
    assert!(
        mounted(&harness, surface),
        "a duration was guessed somewhere"
    );

    // The animation actually ends.
    harness.window.host.set_running_animations(node, 0);
    harness.window.dispatcher().send_to(
        node,
        EventKind::AnimationEnd,
        Payload::Animation(AnimationEvent {
            name: zgui::view::Ident::new("exit"),
            elapsed: Duration::from_millis(180),
            phase: AnimationPhase::Ended,
            pseudo: None,
        }),
    );
    harness.window.frame();
    assert!(!mounted(&harness, surface));
}

#[test]
fn a_second_animation_still_running_keeps_the_content_mounted() {
    // A surface that fades and slides has two animations. Unmounting on the first end would cut
    // the other one off, and the count is what tells them apart.
    let (harness, surface, present) = opened();
    let node = surface.get_untracked().expect("bound");

    present.set(false);
    harness.window.frame();
    harness.window.host.set_running_animations(node, 2);
    harness.window.advance(Duration::ZERO);

    // One of the two ends; the other is still going.
    harness.window.host.set_running_animations(node, 1);
    harness.window.dispatcher().send_to(
        node,
        EventKind::AnimationEnd,
        Payload::Animation(AnimationEvent {
            name: zgui::view::Ident::new("fade"),
            elapsed: Duration::from_millis(180),
            phase: AnimationPhase::Ended,
            pseudo: None,
        }),
    );
    harness.window.frame();
    assert!(mounted(&harness, surface));

    harness.window.host.set_running_animations(node, 0);
    harness.window.dispatcher().send_to(
        node,
        EventKind::AnimationEnd,
        Payload::Animation(AnimationEvent {
            name: zgui::view::Ident::new("slide"),
            elapsed: Duration::from_millis(180),
            phase: AnimationPhase::Ended,
            pseudo: None,
        }),
    );
    harness.window.frame();
    assert!(!mounted(&harness, surface));
}

#[test]
fn a_cancelled_animation_still_lets_the_content_go() {
    // An animation that is cancelled produces no end, and content waiting for one would stay
    // mounted for ever.
    let (harness, surface, present) = opened();
    let node = surface.get_untracked().expect("bound");

    present.set(false);
    harness.window.frame();
    harness.window.host.set_running_animations(node, 1);
    harness.window.advance(Duration::ZERO);
    assert!(mounted(&harness, surface));

    harness.window.host.set_running_animations(node, 0);
    harness.window.dispatcher().send_to(
        node,
        EventKind::AnimationCancel,
        Payload::Animation(AnimationEvent {
            name: zgui::view::Ident::new("exit"),
            elapsed: Duration::from_millis(20),
            phase: AnimationPhase::Cancelled,
            pseudo: None,
        }),
    );
    harness.window.frame();
    assert!(!mounted(&harness, surface));
}

#[test]
fn an_exit_whose_end_never_arrives_finishes_anyway() {
    // The failure this is the net for: an animation is running, and no end, cancel or transition
    // event for it ever arrives. Waiting for one for ever is a modal surface that stays over the
    // window, so the dismissal that was asked for happens late instead of never.
    let (harness, surface, present) = opened();
    let node = surface.get_untracked().expect("bound");

    present.set(false);
    harness.window.frame();
    harness.window.host.set_running_animations(node, 1);
    harness.window.advance(Duration::ZERO);
    assert!(mounted(&harness, surface), "the exit was given its chance");

    // Nothing else happens: no end, no cancel, and the engine goes on reporting the animation as
    // running. Only the clock moves.
    harness.window.advance(Duration::from_millis(1_200));

    assert!(
        !mounted(&harness, surface),
        "the content was asked to go and never went, so the window is still covered by it"
    );
}

#[test]
fn an_exit_that_is_reopened_is_not_taken_away_by_the_deadline_it_armed() {
    // The deadline belongs to one dismissal. Content that closed and opened again inside it must
    // not be unmounted by the timer the closing armed — which would take a surface off the screen
    // a second after the user opened it, with nothing having asked for that.
    let (harness, surface, present) = opened();
    let node = surface.get_untracked().expect("bound");

    present.set(false);
    harness.window.frame();
    harness.window.host.set_running_animations(node, 1);
    harness.window.advance(Duration::ZERO);

    present.set(true);
    harness.window.frame();
    harness.window.advance(Duration::from_millis(1_200));

    assert!(mounted(&harness, surface), "it is open");
    assert_eq!(state(&harness, surface).as_deref(), Some("open"));
}

#[test]
fn reopening_before_the_exit_finishes_keeps_the_content_where_it_is() {
    let (harness, surface, present) = opened();
    let node = surface.get_untracked().expect("bound");

    present.set(false);
    harness.window.frame();
    harness.window.host.set_running_animations(node, 1);
    harness.window.advance(Duration::ZERO);

    present.set(true);
    harness.window.frame();
    assert_eq!(state(&harness, surface).as_deref(), Some("open"));

    // The interrupted exit animation ends. The content stays, because it is open again.
    harness.window.host.set_running_animations(node, 0);
    harness.window.dispatcher().send_to(
        node,
        EventKind::AnimationEnd,
        Payload::Animation(AnimationEvent {
            name: zgui::view::Ident::new("exit"),
            elapsed: Duration::from_millis(90),
            phase: AnimationPhase::Ended,
            pseudo: None,
        }),
    );
    harness.window.frame();
    assert!(mounted(&harness, surface));
}
