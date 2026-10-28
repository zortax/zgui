//! A restyle that changes only how a warm paragraph wraps reaches its lines.
//!
//! `white-space` moves the breaking key and leaves the shaping key alone, so the element owes a
//! rebreak and no reshape. The flattened form of its context holds the runs with their old wrap
//! mode, and the paragraph key is hashed from those runs.

mod support;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

fn pointer_at(y: f32) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::new(CssPx(20.0), CssPx(y))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// How many lines the first inline context under `node` lays out.
fn lines(window: &zgui_runtime::Window, node: NodeRef) -> usize {
    let layout = window.layout().borrow();
    let id = node.get().expect("the label is mounted");
    let key = zgui_view_dom::id::to_document(id).expect("a document node");
    let mut stack: Vec<zgui_layout::BoxKey> = layout.boxes_of(key).to_vec();
    while let Some(box_) = stack.pop() {
        if let Some(resolution) = layout.inline_resolution(box_) {
            return resolution.lines.len();
        }
        stack.extend(layout.node(box_).children.iter().copied());
    }
    panic!("the label laid out no line");
}

/// Line counts at rest, while the row is hovered, at rest again and hovered again.
fn check(rest: &str, hovered: &str) -> [usize; 4] {
    let css = format!(
        "root {{ display: block; width: 300px; height: 300px }}
         .row {{ display: block; width: 60px; height: 200px }}
         .word {{ display: block; {rest} }}
         .row:hover .word {{ {hovered} }}"
    );
    let label = NodeRef::new();
    let mut app = support::app_with_text(&css, move |cx: &mut BuildCx<'_>| {
        let row = zgui_elements::column().class("row").child(
            zgui_elements::label()
                .class("word")
                .node_ref(label)
                .child("one two three four five"),
        );
        Box::new(row.into_view().build(cx))
    });
    let mut counts = [0; 4];
    app.settle(8);
    counts[0] = lines(&app.app().windows()[0], label);
    for (step, y) in [10.0, 280.0, 10.0].into_iter().enumerate() {
        app.deliver_to_first(pointer_at(y));
        app.settle(8);
        counts[step + 1] = lines(&app.app().windows()[0], label);
    }
    app.shut_down();
    counts
}

#[test]
fn a_nowrap_label_wraps_each_time_a_restyle_lets_it() {
    let [rest, hovered, rest_again, hovered_again] =
        check("white-space: nowrap", "white-space: normal");
    assert_eq!(rest, 1, "a nowrap label keeps one line");
    assert!(
        hovered > 1,
        "the hovered label wraps at 60px: {hovered} line(s)"
    );
    assert_eq!(rest_again, 1, "the label is nowrap again");
    assert_eq!(hovered_again, hovered, "the label wraps again");
}

#[test]
fn a_wrapping_label_stops_each_time_a_restyle_tells_it() {
    let [rest, hovered, rest_again, hovered_again] =
        check("white-space: normal", "white-space: nowrap");
    assert!(rest > 1, "the label wraps at 60px: {rest} line(s)");
    assert_eq!(hovered, 1, "the hovered label is nowrap");
    assert_eq!(rest_again, rest, "the label wraps again");
    assert_eq!(hovered_again, 1, "the hovered label is nowrap again");
}
