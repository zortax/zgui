//! What a scroll is allowed to do to the structure that answers what is under the pointer.
//!
//! # The defect this is written against
//!
//! Every fragment under a scroller moves on every frame of a scroll, and each one was taken out of
//! the spatial hierarchy and searched back into it — a descent from the root looking for a node
//! whose rectangle contained the old envelope, allocating at every level to escape a borrow. Two
//! thirds of the fragment stage of a wheel notch went into that search, and it arrived where it
//! started: a scroll moves neighbours together, so the grouping the search rediscovers is the
//! grouping the hierarchy already had.
//!
//! Making it cheaper makes the engine do less, and the whole failure mode of doing less here is
//! silent. An entry rewritten where it lies under a node whose rectangle no longer contains it is
//! never found again: the click goes through it to whatever is behind, and nothing anywhere reports
//! a fault. So the work is asserted against controls, and beside it the answers themselves — every
//! row of a scrolled list is clicked, at the place the fragment tree says it now is, and the
//! element the click reached is checked against the element that owns that place.
//!
//! It is its own test target because the counters are one process-wide block.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_profile::Counter;
use zgui_testkit_scene::counters::Recording;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{
    Modifiers, PointerAction, PointerEvent, ScrollDelta, ScrollPhase, Timestamp, WheelEvent,
};

/// A list of fixed-height rows inside a scrollport a fraction of their height.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
";

/// How many rows the scroller holds, which is many more than fit in it.
const ROWS: usize = 60;

/// How many frames of motion one wheel notch is followed for.
const FRAMES: usize = 24;

/// Which row each click reached, in the order the clicks were made.
type Clicked = Rc<RefCell<Vec<usize>>>;

/// A window holding one scroll container whose every row records being clicked.
fn listing(clicked: &Clicked) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let clicked = Rc::clone(clicked);
    support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let mut port = zgui_elements::column().class("port");
        for index in 0..ROWS {
            let reached = Rc::clone(&clicked);
            port = port.child(zgui_elements::column().class("row").on(
                zgui_view::events::CLICK,
                move |_| {
                    reached.borrow_mut().push(index);
                },
            ));
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

/// Turns the wheel over the container and lets the detent arrive.
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
    for _ in 0..FRAMES {
        harness.advance(std::time::Duration::from_millis(20));
        harness.pump();
    }
    harness.settle(4);
}

/// Clicks once at `y`, in CSS pixels down the window.
fn click(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, y: f32) {
    for action in [
        PointerAction::Moved,
        PointerAction::Pressed,
        PointerAction::Released,
    ] {
        harness.deliver_to_first(SurfaceEvent::Pointer {
            action,
            event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(y))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }
    harness.settle(2);
}

/// Every twenty-pixel row's vertical extent in device pixels, in the order the fragments hold them.
///
/// This is the independent answer the clicks are checked against: it comes out of the fragment
/// tree, which is what the frame *drew*, and never out of the index that is being tested.
fn row_extents(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> Vec<(f32, f32)> {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    let mut rows: Vec<(f32, f32)> = Vec::new();
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            // A row, not a piece of the scrollport's own chrome. A thumb clamped to its minimum
            // length is exactly as tall as a row of this fixture, and one sorted in among them
            // would shift every row's index by one.
            if matches!(fragment.kind, zgui_layout::FragmentKind::Scrollbar { .. }) {
                continue;
            }
            if (fragment.border_box.size.height.0 - 20.0).abs() > 0.5 {
                continue;
            }
            let top = fragment.border_box.origin.y.0;
            rows.push((top, top + fragment.border_box.size.height.0));
        }
    }
    rows.sort_by(|one, two| one.0.total_cmp(&two.0));
    rows
}

/// Which row index covers `y`, counting rows from the top of the list.
///
/// The rows are all the same height and are laid out in order, so the row's position in the sorted
/// extents *is* its index — which is what makes the answer independent of the hit index.
fn row_at(extents: &[(f32, f32)], y: f32) -> Option<usize> {
    extents
        .iter()
        .position(|(top, bottom)| y >= *top && y < *bottom)
}

#[test]
fn a_click_after_a_scroll_lands_where_the_fragment_tree_says_it_should() {
    // This case reads no counter and drives a whole second application, so it bumps every counter
    // the case below is measuring. A recording is taken for the exclusion alone: without it the two
    // run side by side and the numbers the other case reads are the sum of two documents, which is
    // a control that passes because of the run beside it rather than because of the run it is a
    // control for.
    let _exclusive = Recording::begin();
    let clicked: Clicked = Rc::new(RefCell::new(Vec::new()));
    let mut harness = listing(&clicked);
    harness.settle(8);

    // Before the scroll: every point inside the port is clicked, and each has to reach the row the
    // fragments put there. This is the control for the same sweep after the scroll — a run in which
    // the index answered nothing at all would produce an empty log here too.
    let mut expected: Vec<usize> = Vec::new();
    let extents = row_extents(&harness);
    for step in 0..24 {
        let y = 2.0 + step as f32 * 5.0;
        let Some(row) = row_at(&extents, y) else {
            continue;
        };
        expected.push(row);
        click(&mut harness, y);
    }
    assert!(
        expected.len() > 8,
        "the sweep has to cross several rows to mean anything, and crossed {}",
        expected.len()
    );
    assert_eq!(*clicked.borrow(), expected, "before any scrolling");

    clicked.borrow_mut().clear();
    let settled = row_extents(&harness);
    wheel(&mut harness, 3.0);
    let scrolled = row_extents(&harness);
    assert!(
        scrolled[0].0 < settled[0].0 - 1.0,
        "the content did not move, so a sweep that still worked proves nothing: \
         {settled:?} to {scrolled:?}"
    );

    let mut after: Vec<usize> = Vec::new();
    for step in 0..24 {
        let y = 2.0 + step as f32 * 5.0;
        let Some(row) = row_at(&scrolled, y) else {
            continue;
        };
        after.push(row);
        click(&mut harness, y);
    }
    assert_eq!(*clicked.borrow(), after, "after the wheel notch");
    assert_ne!(
        after, expected,
        "the same points reached the same rows, so the list did not really move under them"
    );
}

#[test]
fn a_scrolled_fragment_keeps_the_place_it_had_in_the_hierarchy() {
    let clicked: Clicked = Rc::new(RefCell::new(Vec::new()));
    let mut recording = Recording::begin();
    let mut harness = listing(&clicked);

    // The control: building the document puts every entry into the hierarchy from nothing, which
    // is what makes "hardly any went back through it" mean something rather than meaning that no
    // entry was ever placed.
    let built = recording.measure(|| {
        harness.settle(8);
    });
    let rebuilds = built.control(Counter::HitIndexRebuilds);
    // Measured against the document rather than against a round number: every row generates a
    // fragment and so does the port around them, so a build that placed fewer entries than there
    // are rows placed nothing worth comparing against. The build's entries reach the hierarchy
    // through its one bulk rebuild — a fresh document crosses the churn threshold immediately,
    // and per-entry placement on the way to an owed rebuild is thrown-away work.
    assert!(
        built.get(Counter::HitEntriesUpdated) > ROWS as u64,
        "the build did not write entries at all, so the comparison below has no control: {}",
        built.get(Counter::HitEntriesUpdated)
    );
    assert!(
        built.get(Counter::HitIndexRebuilds) >= 1,
        "the build's entries never reached the hierarchy: no bulk build ran"
    );

    let before = row_extents(&harness);
    let scrolled = recording.measure(|| {
        wheel(&mut harness, 3.0);
    });
    let after = row_extents(&harness);
    assert!(
        after[0].0 < before[0].0 - 1.0,
        "a scroll that moved nothing touches no entry and satisfies every budget here"
    );

    let updated = scrolled.get(Counter::HitEntriesUpdated);
    let kept = scrolled.get(Counter::HitEntriesMovedInPlace);
    let reinserted = scrolled.get(Counter::HitEntriesReinserted);
    assert!(
        updated > 100,
        "the scroll did not move any indexed fragment, so nothing below is being measured"
    );
    assert_eq!(
        kept + reinserted,
        updated,
        "every write took one of the two"
    );
    assert!(
        reinserted * 4 < kept,
        "a scroll moves neighbours together and should keep them together: \
         {kept} rewritten where they lay, {reinserted} searched for a new home"
    );
    scrolled.assert_zero(Counter::HitIndexRebuilds, &rebuilds);
}
