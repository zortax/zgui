//! A popper driven through real frames, with real measurements delivered to it.

mod harness;

use harness::{Harness, content_size, rect};
use zgui::prelude::*;
use zgui::view::{Dom, ObservedValue};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

/// A trigger and a popper anchored to it, both handles handed back to the test.
#[component]
fn Anchored(
    /// The element the surface is placed against.
    anchor: NodeRef,
    /// The positioner.
    positioner: NodeRef,
    /// Where the surface is asked to go.
    #[prop(default = Placement::BOTTOM)]
    placement: Placement,
    /// Whether it may cross to the other side.
    #[prop(default = true)]
    flip: bool,
) -> impl IntoView {
    view! {
        box {
            control(node_ref = anchor) {"open"}
            Popper(anchor = anchor, element_ref = positioner, placement = placement, flip = flip) {
                box(class = "menu") {"contents"}
            }
        }
    }
}

/// Mounts a popper, and hands back the harness plus the two handles.
fn open(placement: Placement, flip: bool) -> (Harness, NodeRef, NodeRef) {
    let harness = Harness::open();
    let anchor = harness.window.scope.with(NodeRef::new);
    let positioner = harness.window.scope.with(NodeRef::new);
    harness.mount(move || {
        view! { Anchored(anchor = anchor, positioner = positioner, placement = placement, flip = flip) }
    });
    (harness, anchor, positioner)
}

/// What the positioner's inline style says now.
fn placed(
    harness: &Harness,
    positioner: NodeRef,
) -> (Option<String>, Option<String>, Option<String>) {
    let node = positioner.get_untracked().expect("the positioner is bound");
    let tree = harness.window.dom.tree();
    (
        tree.style_property(node, "left"),
        tree.style_property(node, "top"),
        tree.style_property(node, "visibility"),
    )
}

/// The `data-` attribute the style sheet selects on.
fn attribute(harness: &Harness, positioner: NodeRef, name: &str) -> Option<String> {
    let node = positioner.get_untracked().expect("the positioner is bound");
    harness
        .window
        .dom
        .tree()
        .attribute(node, zgui::view::AttrName::new(name))
}

#[test]
fn a_popper_is_hidden_until_it_has_been_measured() {
    // The alternative is a surface drawn at the anchor's corner for one frame and moved on the
    // next, and no later correction takes that frame back.
    let (harness, _anchor, positioner) = open(Placement::BOTTOM, true);

    let (left, top, visibility) = placed(&harness, positioner);
    assert_eq!(
        left, None,
        "nothing has been measured, so nothing is placed"
    );
    assert_eq!(top, None);
    assert_eq!(visibility.as_deref(), Some("hidden"));
    assert_eq!(attribute(&harness, positioner, "data-side"), None);
}

#[test]
fn a_popper_near_the_bottom_edge_is_placed_above_in_the_frame_it_opened() {
    // One delivery of the three measurements, one flush, and the surface is in its final place
    // with its visibility cleared. Everything about this test is that there is no second round.
    let (harness, anchor, positioner) = open(Placement::BOTTOM, true);
    let anchor_node = anchor.get_untracked().expect("bound");
    let positioner_node = positioner.get_untracked().expect("bound");
    let root = harness.window.dom.root(positioner_node);

    harness
        .window
        .dom
        .deliver(root, ObservedValue::BorderBox(rect(0.0, 0.0, 800.0, 600.0)));
    harness.window.dom.deliver(
        anchor_node,
        ObservedValue::BorderBox(rect(100.0, 560.0, 80.0, 24.0)),
    );
    harness
        .window
        .dom
        .deliver(positioner_node, content_size(200.0, 160.0));
    harness.window.frame();

    assert_eq!(
        attribute(&harness, positioner, "data-side").as_deref(),
        Some("top"),
        "there is no room below, so it went above"
    );
    assert_eq!(
        attribute(&harness, positioner, "data-align").as_deref(),
        Some("center")
    );

    let (left, top, visibility) = placed(&harness, positioner);
    assert_eq!(visibility, None, "it is visible in the frame it opened");
    let top: f32 = top
        .expect("placed")
        .trim_end_matches("px")
        .parse()
        .expect("a pixel length");
    let left: f32 = left
        .expect("placed")
        .trim_end_matches("px")
        .parse()
        .expect("a pixel length");
    assert!(
        top >= 0.0 && top + 160.0 <= 600.0,
        "the surface is inside the window: top {top}"
    );
    assert!(left >= 0.0 && left + 200.0 <= 800.0, "left {left}");
}

#[test]
fn delivering_the_same_measurements_again_writes_nothing_more() {
    // The property behind "converges in one pass": a second delivery of values that did not change
    // produces no further writes, so a document with a popper in it settles.
    let (harness, anchor, positioner) = open(Placement::BOTTOM, true);
    let anchor_node = anchor.get_untracked().expect("bound");
    let positioner_node = positioner.get_untracked().expect("bound");
    let root = harness.window.dom.root(positioner_node);

    let deliver = || {
        harness
            .window
            .dom
            .deliver(root, ObservedValue::BorderBox(rect(0.0, 0.0, 800.0, 600.0)));
        harness.window.dom.deliver(
            anchor_node,
            ObservedValue::BorderBox(rect(100.0, 100.0, 80.0, 24.0)),
        );
        harness
            .window
            .dom
            .deliver(positioner_node, content_size(200.0, 160.0));
        harness.window.frame();
    };

    deliver();
    let after_first = harness.window.transcript.len();
    deliver();
    assert_eq!(
        harness.window.transcript.len(),
        after_first,
        "a second delivery of the same measurements changed nothing"
    );
}

#[test]
fn a_popper_told_not_to_flip_stays_where_it_was_asked() {
    let (harness, anchor, positioner) = open(Placement::BOTTOM, false);
    let anchor_node = anchor.get_untracked().expect("bound");
    let positioner_node = positioner.get_untracked().expect("bound");
    let root = harness.window.dom.root(positioner_node);

    harness
        .window
        .dom
        .deliver(root, ObservedValue::BorderBox(rect(0.0, 0.0, 800.0, 600.0)));
    harness.window.dom.deliver(
        anchor_node,
        ObservedValue::BorderBox(rect(100.0, 560.0, 80.0, 24.0)),
    );
    harness
        .window
        .dom
        .deliver(positioner_node, content_size(200.0, 160.0));
    harness.window.frame();

    assert_eq!(
        attribute(&harness, positioner, "data-side").as_deref(),
        Some("bottom"),
        "it was told not to flip, so it hangs off the bottom rather than moving"
    );
}

#[test]
fn moving_the_anchor_moves_the_surface_with_it() {
    let (harness, anchor, positioner) = open(Placement::BOTTOM, true);
    let anchor_node = anchor.get_untracked().expect("bound");
    let positioner_node = positioner.get_untracked().expect("bound");
    let root = harness.window.dom.root(positioner_node);

    harness
        .window
        .dom
        .deliver(root, ObservedValue::BorderBox(rect(0.0, 0.0, 800.0, 600.0)));
    harness
        .window
        .dom
        .deliver(positioner_node, content_size(100.0, 50.0));
    harness.window.dom.deliver(
        anchor_node,
        ObservedValue::BorderBox(rect(100.0, 100.0, 80.0, 24.0)),
    );
    harness.window.frame();
    let (_, first, _) = placed(&harness, positioner);

    // Something scrolled: the anchor is somewhere else now.
    harness.window.dom.deliver(
        anchor_node,
        ObservedValue::BorderBox(rect(100.0, 300.0, 80.0, 24.0)),
    );
    harness.window.frame();
    let (_, second, _) = placed(&harness, positioner);

    assert_ne!(first, second);
    assert_eq!(
        second.as_deref(),
        Some("328px"),
        "300 + 24 + the 4px offset"
    );
}
