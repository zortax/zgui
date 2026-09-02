//! A status label beside a hover-revealed control keeps its words after the pointer has left.
//!
//! The shape of a thread row in a sidebar: a name that shrinks, a filler, a status made of an
//! icon and a nowrap label, and a set of quick actions that only show while the row is hovered.
//! Hovering swaps the two, leaving swaps them back, and the status must come back whole.

mod support;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

const ROWS: usize = 3;
const ROW_HEIGHT: f32 = 40.0;

/// The sidebar sheet, with the status/quick swap expressed by `swap`.
fn css(swap: &str) -> String {
    format!(
        "root {{ display: block; width: 280px; height: 300px }}
         .side {{ display: flex; flex-direction: column; width: 280px }}
         .row {{ display: flex; flex-direction: column; height: {ROW_HEIGHT}px; padding: 4px 8px }}
         .top {{ display: flex; flex-direction: row; align-items: center; gap: 6px }}
         .name {{ flex: 1 1 auto; min-width: 0; white-space: nowrap; overflow: hidden;
                  text-overflow: ellipsis }}
         .fill {{ flex: 1 1 auto }}
         .state {{ display: flex; flex-direction: row; flex: 0 0 auto; align-items: center;
                   gap: 5px }}
         .icon {{ width: 12px; height: 12px; flex: 0 0 auto }}
         .word {{ white-space: nowrap; overflow: hidden; text-overflow: ellipsis }}
         .quick {{ display: flex; flex-direction: row; flex: 0 0 auto; gap: 2px }}
         .act {{ display: flex; padding: 0 5px }}
         {swap}"
    )
}

const SWAP_BY_DISPLAY: &str = ".quick { display: none }
                              .row:hover .quick { display: flex }
                              .row:hover .state { display: none }";

const SWAP_BY_WIDTH: &str = ".state, .quick { overflow: hidden; white-space: nowrap }
                            .quick { width: 0 }
                            .row:hover .quick { width: auto }
                            .row:hover .state { width: 0; gap: 0 }";

fn pointer_at(y: f32) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::new(CssPx(100.0), CssPx(y))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// The width of the first line laid out under `node`, and whether it was cut.
fn status_line(window: &zgui_runtime::Window, node: NodeRef) -> (f32, bool) {
    let layout = window.layout().borrow();
    let id = node.get().expect("the label is mounted");
    let key = zgui_view_dom::id::to_document(id).expect("a document node");
    let mut stack: Vec<zgui_layout::BoxKey> = layout.boxes_of(key).to_vec();
    while let Some(box_) = stack.pop() {
        if let Some(resolution) = layout.inline_resolution(box_) {
            let line = &resolution.lines[0];
            return (line.width, line.ellipsis.is_some());
        }
        stack.extend(layout.node(box_).children.iter().copied());
    }
    panic!("the label laid out no line");
}

fn check(swap: &'static str) {
    let labels: Vec<NodeRef> = (0..ROWS).map(|_| NodeRef::new()).collect();
    let handles = labels.clone();
    let mut app = support::app_with_text(&css(swap), move |cx: &mut BuildCx<'_>| {
        let mut side = zgui_elements::column().class("side");
        for handle in &handles {
            side = side.child(
                zgui_elements::column().class("row").child(
                    zgui_elements::row()
                        .class("top")
                        .child(zgui_elements::label().class("name").child("A thread title"))
                        .child(zgui_elements::r#box().class("fill"))
                        .child(
                            zgui_elements::row()
                                .class("state")
                                .child(zgui_elements::r#box().class("icon"))
                                .child(
                                    zgui_elements::label()
                                        .class("word")
                                        .node_ref(*handle)
                                        .child("idle"),
                                ),
                        )
                        .child(
                            zgui_elements::row().class("quick").child(
                                zgui_elements::control()
                                    .class("act")
                                    .child(zgui_elements::label().class("word").child("snooze")),
                            ),
                        ),
                ),
            );
        }
        Box::new(side.into_view().build(cx))
    });
    app.settle(8);
    let full = status_line(&app.app().windows()[0], labels[0]);
    assert!(!full.1, "at rest the status is whole: {full:?}");

    // Hover the middle row, then leave it for the last one, then leave every row.
    app.deliver_to_first(pointer_at(ROW_HEIGHT * 1.5));
    app.settle(8);
    app.deliver_to_first(pointer_at(ROW_HEIGHT * 2.5));
    app.settle(8);
    app.deliver_to_first(pointer_at(ROW_HEIGHT * 5.0));
    app.settle(8);

    for (index, label) in labels.iter().enumerate() {
        let after = status_line(&app.app().windows()[0], *label);
        assert_eq!(
            after, full,
            "row {index}: the status came back {after:?}, and at rest it was {full:?}"
        );
    }
    app.shut_down();
}

#[test]
fn a_status_hidden_by_display_comes_back_whole() {
    check(SWAP_BY_DISPLAY);
}

#[test]
fn a_status_folded_to_no_width_comes_back_whole() {
    check(SWAP_BY_WIDTH);
}
