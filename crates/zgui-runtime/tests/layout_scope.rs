//! What a frame is allowed to lay out, and what it must still put on the screen.
//!
//! # The defect this is written against
//!
//! Laying a document out was unconditional. Every frame ran the intrinsic pre-pass and the root
//! layout from the root, whatever had changed and whether anything had — so a hover that recoloured
//! one button, and an animation that repaints a placeholder sixty times a second for ever, each
//! walked every box in the document to reach per-box caches that all hit. On a real page that was
//! forty-three microseconds of a ninety-three microsecond frame, for two boxes actually laid out.
//!
//! And when a pass *was* owed, the engine's own per-box cache kept nine slots chosen by the shape
//! of the question rather than by the question — so the min-content and max-content probes grid
//! track sizing repeats against a moving grid-area estimate evicted each other, and each eviction
//! was a whole nested layout of a panel that had not changed. One keystroke drove nine box layouts
//! per box in the document.
//!
//! Both of those make the engine do less, which is how a frame comes to show what the last one
//! showed. So every case below pairs the counter that proves the saving with the two things a frame
//! that did nothing would fail: the element that changed has to have been repainted, and the
//! rectangle the frame drew against has to cover it.
//!
//! It is its own test target because the counters are one process-wide block.

mod support;

use zgui_geom::{Css, CssPx, Device, Point, Rect};
use zgui_platform::SurfaceEvent;
use zgui_profile::Counter;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_testkit_scene::counters::Recording;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// A three-column grid of panels, which is the shape that makes the probing visible, with one
/// swatch above it that a pointer can be moved onto.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.swatch { display: block; width: 34px; height: 34px; background-color: #202020 }
.swatch:hover { background-color: #f0f0f0 }
.grid { display: grid; grid-template-columns: auto auto auto; width: 400px }
.panel { display: block }
.heading { display: block; font-size: 20px }
.row { display: flex; flex-direction: row }
.cell { display: flex; flex-direction: column; flex-grow: 1 }
.body { display: block; font-size: 12px }
.label { display: inline-block; font-size: 12px }
";

/// How many panels the grid holds.
const PANELS: usize = 9;

/// A window holding the swatch, the grid, and one label whose characters a signal writes.
fn page(typed: RwSignal<String>) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let mut grid = zgui_elements::column().class("grid");
        // Inside the grid, because what the memo is for is the re-measurement of every *other*
        // panel that one panel's change sets off.
        grid = grid.child(
            zgui_elements::column().class("panel").child(
                zgui_elements::column()
                    .class("label")
                    .child(move || typed.get()),
            ),
        );
        for _ in 0..PANELS {
            grid = grid.child(
                zgui_elements::column()
                    .class("panel")
                    .child(
                        zgui_elements::column()
                            .class("heading")
                            .child("a heading of some length"),
                    )
                    .child(
                        zgui_elements::column()
                            .class("row")
                            .child(zgui_elements::column().class("cell").child(
                                zgui_elements::column().class("body").child(
                                    "a paragraph with rather more words in it than the heading",
                                ),
                            ))
                            .child(zgui_elements::column().class("cell").child(
                                zgui_elements::column().class("body").child("a shorter one"),
                            )),
                    ),
            );
        }
        Box::new(
            zgui_elements::column()
                .child(zgui_elements::column().class("swatch"))
                .child(grid)
                .into_view()
                .build(cx),
        )
    })
}

/// Moves the pointer to a point in the window and lets the frame it asks for run.
fn point_at(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    at: Point<CssPx, Css>,
) {
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(at),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);
}

/// How many boxes the document holds altogether.
fn boxes(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> u64 {
    let window = harness.app().windows().first().expect("a window");
    u64::from(window.layout().borrow().len())
}

/// The border box of the first fragment of the element carrying `class`.
///
/// # Panics
///
/// Panics when nothing in the document carries the class, because every caller is about to assert
/// something about where it is.
fn box_of(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    class: &str,
) -> Rect<zgui_geom::DevicePx, Device> {
    let window = harness.app().windows().first().expect("a window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    for key in layout.keys() {
        let Some(source) = layout.node(key).source else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| held.0.as_ref() == class)
        {
            continue;
        }
        if let Some(&frag) = layout.fragments_of_box(key).first()
            && let Some(fragment) = layout.fragment(frag)
        {
            return fragment.border_box;
        }
    }
    panic!("nothing in the document carries `{class}`")
}

/// How wide the widest line of text under the element carrying `class` came out.
///
/// The line rather than the box: a block inside a column is as wide as the column whatever it
/// holds, and what a keystroke moves is the extent of the characters themselves.
///
/// # Panics
///
/// Panics when nothing in the document carries the class, or when nothing under it holds a line.
fn line_width_under(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    class: &str,
) -> f32 {
    let window = harness.app().windows().first().expect("a window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    for key in layout.keys() {
        let Some(source) = layout.node(key).source else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| held.0.as_ref() == class)
        {
            continue;
        }
        let mut widest: Option<f32> = None;
        let mut stack = vec![key];
        while let Some(current) = stack.pop() {
            for &frag in layout.fragments_of_box(current) {
                let Some(fragment) = layout.fragment(frag) else {
                    continue;
                };
                if matches!(
                    fragment.kind,
                    zgui_layout::fragment::FragmentKind::Line { .. }
                ) {
                    let width = fragment.border_box.size.width.0;
                    widest = Some(widest.map_or(width, |held: f32| held.max(width)));
                }
            }
            stack.extend(layout.node(current).children.iter().copied());
        }
        if let Some(width) = widest {
            return width;
        }
    }
    panic!("nothing carrying `{class}` holds a line of text")
}

/// A frame that changed one element's paint does not lay the document out.
///
/// The saving is `layouts_held`, and it is paired with the two things a frame that did nothing at
/// all would also satisfy: the swatch has to have been repainted, and the box tree has to still be
/// the box tree — a rebuild would relayout for reasons of its own and the counter would be right
/// for the wrong reason.
#[test]
fn a_hover_holds_the_layout_and_still_repaints() {
    let mut recording = Recording::begin();
    let typed = RwSignal::new(String::from("ab"));
    let mut harness = page(typed);

    // The control: opening the window lays every box out, which is what makes the zero below mean
    // "this frame did not" rather than "nothing here ever counts a layout".
    let built = recording.measure(|| {
        harness.settle(16);
    });
    let laid_out = built.control(Counter::NodesRelaidOut);
    assert!(
        laid_out.value() > boxes(&harness),
        "opening the window laid out {} boxes for a document of {}, so the control is not a \
         document-wide layout",
        laid_out.value(),
        boxes(&harness)
    );

    let swatch = box_of(&harness, "swatch");
    let hovered = recording.measure(|| {
        point_at(
            &mut harness,
            Point::new(
                CssPx(swatch.origin.x.0 + swatch.size.width.0 / 2.0),
                CssPx(swatch.origin.y.0 + swatch.size.height.0 / 2.0),
            ),
        );
    });

    // The positive control: the hover reached the cascade. A pointer that landed on nothing would
    // hold the layout too, and would prove nothing.
    assert!(
        hovered.get(Counter::ElementsRestyled) > 0,
        "the pointer landed on nothing, so the frame below is an idle one"
    );
    assert!(
        hovered.get(Counter::Repaints) > 0,
        "the swatch changed colour and nothing was repainted"
    );
    assert!(
        hovered.get(Counter::LayoutsHeld) > 0,
        "the frame ran a layout pass for a change that moved no box"
    );
    hovered.assert_zero(Counter::NodesRelaidOut, &laid_out);
    assert_eq!(
        hovered.get(Counter::BoxesRebuilt),
        0,
        "the hover rebuilt boxes, so the layout it held was held for the wrong reason"
    );
}

/// An idle animation frame repaints without laying anything out.
///
/// A page holding a placeholder or an indeterminate progress bar produces a frame every refresh
/// interval for ever. Each of those used to be a whole-document layout for one restyled element.
#[test]
fn an_idle_frame_holds_the_layout() {
    let mut recording = Recording::begin();
    let typed = RwSignal::new(String::from("ab"));
    let mut harness = page(typed);
    harness.settle(16);

    let idle = recording.measure(|| {
        for _ in 0..8 {
            harness.advance(std::time::Duration::from_millis(17));
            harness.pump();
        }
    });
    assert_eq!(
        idle.get(Counter::NodesRelaidOut),
        0,
        "a document nothing touched was laid out again"
    );
    assert_eq!(
        idle.get(Counter::BoxesRebuilt),
        0,
        "a document nothing touched had boxes rebuilt"
    );
}

/// One character typed lays out what changed, not every panel in the grid.
///
/// The pairing is the point: the label has to be wider afterwards — a frame that laid nothing out
/// would satisfy the bound and leave the old characters on the screen at the old width.
#[test]
fn a_keystroke_lays_out_far_less_than_the_document() {
    let mut recording = Recording::begin();
    let typed = RwSignal::new(String::from("ab"));
    let mut harness = page(typed);

    let built = recording.measure(|| {
        harness.settle(16);
    });
    let laid_out = built.control(Counter::NodesRelaidOut);
    let total = boxes(&harness);
    let before = line_width_under(&harness, "label");

    let typing = recording.measure(|| {
        typed.set(String::from("abcdefghijklmnopqrstuvwx"));
        harness.settle(16);
    });
    let after = line_width_under(&harness, "label");

    // The positive control. This is the whole reason the bound below is not satisfied by a frame
    // that did nothing.
    assert!(
        after > before + 1.0,
        "the label's line was {before} wide before the keystroke and {after} after, so nothing \
         was re-measured"
    );
    assert!(
        typing.get(Counter::Repaints) > 0,
        "the characters changed and nothing was repainted"
    );
    // The bound is the document's own box count: a keystroke that re-measured every panel in
    // every column costs several layouts per box, which is what the control above is.
    typing.assert_at_most(Counter::NodesRelaidOut, total, &laid_out);
}
