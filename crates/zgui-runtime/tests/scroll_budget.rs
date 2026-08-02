//! What one scroll of a real document costs, measured against a run that did the work.
//!
//! This is the phase's central claim and it is the one most easily satisfied by accident. Almost
//! every assertion below is that a counter stayed at **zero**, and a zero is equally true of a
//! pipeline that did no restyling and of one that has no restyle stage at all — so each is paired
//! with a control taken from a run in which the same counter moved. The scroll itself has a
//! positive control too: the content is read out of the fragment tree before and after, because a
//! scroll that moved nothing satisfies every budget here.
//!
//! # Why re-encoding is bounded rather than forbidden
//!
//! A row below the port paints nothing while it is down there — every primitive it offers misses
//! the clip and is refused — so what the paint stage records for it is an empty range. That range
//! is the whole of what the row paints outside the port and none of what it paints inside one, so
//! the frame a row *arrives* in has to encode it for the first time. Forbidding that is forbidding
//! the row from being drawn at all, which is the defect this budget used to be satisfied by.
//!
//! A scroll has **two** edges and the same reasoning holds at both. A row that was wholly inside
//! the port and is now half out of it recorded the whole of itself and can no longer replay it, so
//! it is encoded again for as long as it lies across the boundary — and it has to be, because what
//! it draws at the top edge is a different part of itself on every frame. So a row is encoded again
//! where it *crosses* an edge, in either direction, and nowhere else: below the port and above it
//! there is nothing to draw, and wholly inside it the record stands.
//!
//! So the bound is the number of edge crossings a scroll of this length makes — twice the rows it
//! travelled, plus the two rows already lying across the edges when it started — and the claim it
//! makes is the one that matters: a scroll costs the rows that moved past an edge and not the list.
//!
//! It is its own test target because the counters are one process-wide block. Every test in this
//! binary holds a recording, so nothing else in the process is writing to them while it measures.

mod support;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_profile::Counter;
use zgui_testkit_scene::counters::Recording;
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// A list of fixed-height rows inside a scrollport a fraction of their height.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
";

/// A window holding one scroll container with `rows` rows in it.
fn listing(rows: usize) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let mut port = zgui_elements::column().class("port");
        for _ in 0..rows {
            port = port.child(zgui_elements::column().class("row"));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .into_view()
                .build(cx),
        )
    })
}

/// How many frames of motion the helper runs.
const FRAMES: usize = 24;

/// How tall one row is, in device pixels, which is what the sheet says.
const ROW: f32 = 20.0;

/// Turns the wheel `lines` lines over the container, and lets the detent arrive.
///
/// The frames that *carry* the detent are inside this rather than outside it, deliberately. A
/// detent travels to its destination over the following few frames, and each one of those composes
/// fragments again — so the budget below is a claim about every frame of a scroll and not only
/// about the one the event arrived in, which is where the claim would be easiest to satisfy and
/// least worth making.
fn wheel(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, lines: f32) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    // The clock moves with the frames, because that is what a window does: the frame that drains
    // the event is also a frame of the motion the event started. Running frames without moving the
    // clock first would leave the detent sitting still through several repaints, which is not a
    // state a window is ever in.
    for _ in 0..FRAMES {
        harness.advance(std::time::Duration::from_millis(20));
        harness.pump();
    }
    harness.settle(4);
}

/// The top edge of the topmost twenty-pixel row, in device pixels.
fn topmost_row(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> f32 {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    let mut top: Option<f32> = None;
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            // A row, not a piece of the scrollport's own chrome. A thumb clamped to its minimum
            // length is exactly as tall as a row of this fixture and sits at the top of the
            // gutter, so a search by height alone measures the scrollbar and reports that nothing
            // moved however far the list went.
            if matches!(fragment.kind, zgui_layout::FragmentKind::Scrollbar { .. }) {
                continue;
            }
            if (fragment.border_box.size.height.0 - 20.0).abs() > 0.5 {
                continue;
            }
            top = Some(match top {
                Some(held) => held.min(fragment.border_box.origin.y.0),
                None => fragment.border_box.origin.y.0,
            });
        }
    }
    top.expect("the document has rows in it")
}

#[test]
fn scroll_does_not_restyle_relayout_or_reencode() {
    let mut recording = Recording::begin();
    let mut harness = listing(200);

    // The control: building the document does all four kinds of work, which is what makes the
    // four zeroes below mean "this frame did not" rather than "this build never could".
    let built = recording.measure(|| {
        harness.settle(8);
    });
    let restyle = built.control(Counter::ElementsRestyled);
    let relayout = built.control(Counter::NodesRelaidOut);
    let encode = built.control(Counter::ChunksReencoded);
    let rebuild = built.control(Counter::HitIndexRebuilds);

    let before = topmost_row(&harness);
    let scrolled = recording.measure(|| {
        wheel(&mut harness, 3.0);
    });
    let after = topmost_row(&harness);

    assert!(
        after < before - 1.0,
        "the content did not move at all: {before} to {after}. A scroll that does nothing \
         satisfies every budget below, so this is what makes them mean anything"
    );

    scrolled.assert_zero(Counter::ElementsRestyled, &restyle);
    scrolled.assert_zero(Counter::NodesRelaidOut, &relayout);
    // One per edge crossing: every row the travel carried past the bottom edge into the port, and
    // every row it carried past the top edge out of it, plus the row already lying across each
    // edge when the scroll began.
    let crossings = 2 * ((before - after) / ROW).ceil() as u64 + 2;
    scrolled.assert_at_most(Counter::ChunksReencoded, crossings, &encode);
    scrolled.assert_zero(Counter::HitIndexRebuilds, &rebuild);
    assert!(
        scrolled.get(Counter::HitEntriesUpdated) > 0,
        "the hit index was not touched at all, so the fragments it indexes did not move either"
    );
}
