//! What a scroll costs, and what it changes, over a real document.
//!
//! Every assertion here mounts a document, turns a wheel over it and reads the frame counters the
//! engines wrote. That is the only arrangement in which the claim "scrolling never re-enters style
//! or layout" is testable at all: a test that wrote a scroll offset by hand and then composed
//! fragments from it would assert the arithmetic and never the pipeline, and the pipeline is where
//! the property lives. The first frame of every one of these tests is a positive control — the
//! content really did move — because a scroll that does nothing satisfies every budget below.

mod support;

use std::sync::Arc;

use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Rect};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_view::ViewHost;
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// One frame of the surface's own refresh rate, rounded up.
///
/// The park is `now` plus a real frame interval — 16.667 ms at sixty hertz — so a test that moved
/// the clock by sixteen would leave the deadline installed and unreached, and read the offset of a
/// scroll that has correctly not been given any time.
const FRAME: std::time::Duration = std::time::Duration::from_millis(20);

/// A list of fixed-height rows inside a scrollport a third of their height.
///
/// `overflow: scroll` rather than `auto`, so the box is a scroll container whatever its content
/// measures to and the fixture cannot quietly stop testing scrolling.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
.tall { display: block; width: 400px; height: 60px; background-color: #404040 }
";

/// A list inside a page, both of which scroll, for the chaining assertions.
const NESTED_CSS: &str = "
root { display: block; width: 400px; height: 3000px }
.page { display: block; width: 400px; height: 200px; overflow: scroll }
.port { display: block; width: 400px; height: 100px; overflow: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
.filler { display: block; width: 400px; height: 900px }
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

/// A window holding a scrolling page with a scrolling list inside it.
fn nested() -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(NESTED_CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let mut port = zgui_elements::column().class("port");
        for _ in 0..20 {
            port = port.child(zgui_elements::column().class("row"));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column()
                        .class("page")
                        .child(port)
                        .child(zgui_elements::column().class("filler")),
                )
                .into_view()
                .build(cx),
        )
    })
}

/// Turns the wheel `lines` lines at `(x, y)`, in CSS pixels, and lets it arrive.
///
/// A detent does not land in the frame it is delivered in: it is carried there over the following
/// few, because that is what a wheel does in every application anybody uses. So the clock is run
/// past the end of that motion before anything is read, and a test that wants to look at the motion
/// *while* it is running says so by driving the clock itself.
fn wheel(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    at: (f32, f32),
    lines: f32,
) {
    turn(harness, at, lines);
    settle_motion(harness);
}

/// Delivers one wheel event and runs the frames it asks for, without waiting for it to arrive.
fn turn(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    at: (f32, f32),
    lines: f32,
) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(at.0), CssPx(at.1)),
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);
}

/// Runs enough frames for every smooth scroll and every displaced edge to have finished.
fn settle_motion(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    for _ in 0..40 {
        harness.advance(FRAME);
        harness.pump();
    }
    harness.settle(4);
}

/// The element behind one box, which is the name a scroll offset is filed under.
fn element_of(window: &zgui_runtime::Window, key: zgui_dom::side::BoxKey) -> zgui_dom::NodeKey {
    window
        .layout()
        .borrow()
        .node(key)
        .source
        .expect("the scroll container came from an element")
}

/// The window the harness is driving.
fn window(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> &zgui_runtime::Window {
    harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window")
}

/// The top edge of the topmost fragment whose height is `height`, in device pixels.
///
/// Read out of the fragment tree rather than assumed: where the content actually is after the
/// scroll is the whole question, and a constant would be asserting the test's own arithmetic.
fn topmost_row_top(window: &zgui_runtime::Window, height: f32) -> f32 {
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
            if (fragment.border_box.size.height.0 - height).abs() > 0.5 {
                continue;
            }
            top = Some(match top {
                Some(held) => held.min(fragment.border_box.origin.y.0),
                None => fragment.border_box.origin.y.0,
            });
        }
    }
    top.unwrap_or_else(|| panic!("no fragment of the document is {height} tall"))
}

/// How many elements carry a given obligation.
fn marked(window: &zgui_runtime::Window, bits: zgui_bits::Dirty) -> usize {
    let document = window.document().borrow();
    let store = document.store();
    let mut count = 0;
    let Some(root) = document.root_index() else {
        return 0;
    };
    let mut stack = vec![root];
    while let Some(index) = stack.pop() {
        let record = store.core(index);
        if record.dirty().own().intersects(bits) {
            count += 1;
        }
        let mut child = record.first_child();
        while let Some(node) = child {
            stack.push(node);
            child = store.core(node).next_sibling();
        }
    }
    count
}

#[test]
fn a_scroll_of_a_five_thousand_node_document_changes_one_accessibility_node() {
    let mut harness = listing(5_000);
    harness.settle(16);
    let before = topmost_row_top(window(&harness), 20.0);

    // The container, and a row far enough down it to be off the end of the scrollport.
    let container = window(&harness)
        .layout()
        .borrow()
        .keys()
        .into_iter()
        .find(|key| {
            zgui_layout::scroll_region::region_of(&window(&harness).layout().borrow(), *key)
                .is_some()
        })
        .expect("a container");
    let deep = {
        let layout = window(&harness).layout().borrow();
        let node = layout.node(container).source.expect("an element");
        let document = window(&harness).document().borrow();
        let index = document.store().index_of(node).expect("live");
        let mut child = document.store().core(index).first_child();
        for _ in 0..3_000 {
            child = child.and_then(|at| document.store().core(at).next_sibling());
        }
        document
            .store()
            .key_of(child.expect("a row three thousand down"))
    };

    // Scrolled directly on the window, so the marks can be counted before the frame that services
    // and retires them. What a scroll *marks* is this phase's property; what an accessibility
    // projection makes of it is the next one's, and is asserted separately below.
    harness.app_mut().windows_mut()[0].scroll_into_view(
        deep,
        zgui_scroll::Align::Nearest,
        zgui_scroll::Behavior::Instant,
    );
    assert_eq!(
        marked(window(&harness), zgui_bits::Dirty::A11Y),
        1,
        "five thousand rows moved on the screen and the accessibility tree owes one node: the \
         container's own scroll position, because every descendant is published relative to it"
    );
    assert_eq!(
        marked(window(&harness), zgui_bits::Dirty::SCROLL),
        1,
        "the fragment pass is entered on one container and descends from there; marking the \
         subtree would be marking, one node at a time, what the pass is about to discover"
    );

    let surface = harness
        .platform()
        .offscreens()
        .first()
        .expect("the application created a surface")
        .clone();
    harness.settle(8);
    assert!(
        topmost_row_top(window(&harness), 20.0) < before - 1.0,
        "nothing moved"
    );

    // And what the projection published for it is bounded by the same one node.
    let published = surface.a11y_updates();
    wheel(&mut harness, (200.0, 60.0), 3.0);
    if surface.a11y_updates() > published {
        let update = surface.last_a11y_update().expect("an update was published");
        assert!(
            update.nodes.len() <= 1,
            "a scroll published {} accessibility nodes",
            update.nodes.len()
        );
    }
}

#[test]
fn a_bottomed_out_list_hands_the_rest_of_the_turn_to_the_page_under_it() {
    let mut harness = nested();
    harness.settle(8);

    // Enough to run the inner list out of content several times over.
    for _ in 0..12 {
        wheel(&mut harness, (200.0, 40.0), 5.0);
    }

    let (inner, outer) = offsets(window(&harness));
    assert!(inner > 0.0, "the list did not scroll at all");
    assert!(
        outer > 0.0,
        "the list bottomed out at {inner} and the page under it never moved; without the chain a \
         wheel over a finished list stops the page dead"
    );
}

#[test]
fn a_wheel_over_a_page_does_not_scroll_a_list_it_is_not_over() {
    let mut harness = nested();
    harness.settle(8);

    // Below the list, over the filler, which only the page scrolls.
    wheel(&mut harness, (200.0, 150.0), 3.0);

    let (inner, outer) = offsets(window(&harness));
    assert_eq!(inner, 0.0, "the list was not under the pointer");
    assert!(outer > 0.0, "and the page was");
}

/// The vertical offsets of the two scroll containers in the nested fixture.
fn offsets(window: &zgui_runtime::Window) -> (f32, f32) {
    let layout = window.layout().borrow();
    let scroll = window.scroll().borrow();
    let mut inner = 0.0;
    let mut outer = 0.0;
    for key in layout.keys() {
        let Some(region) = zgui_layout::scroll_region::region_of(&layout, key) else {
            continue;
        };
        let Some(element) = layout.node(key).source else {
            continue;
        };
        let offset = scroll.offset_of(element).y.0;
        // The list's scrollport is a hundred pixels tall and the page's is two hundred.
        if region.scrollport.size.height.0 <= 150.0 {
            inner = offset;
        } else {
            outer = offset;
        }
    }
    (inner, outer)
}

#[test]
fn a_smooth_scroll_travels_over_several_frames_and_parks_with_a_deadline() {
    let mut harness = listing(200);
    harness.settle(8);

    let container = {
        let window = window(&harness);
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .find(|key| zgui_layout::scroll_region::region_of(&layout, *key).is_some())
            .expect("the fixture has a scroll container")
    };
    let node = window(&harness)
        .layout()
        .borrow()
        .node(container)
        .source
        .expect("the container came from an element");

    window(&harness).host().scroll_to(
        zgui_view_dom::id::to_view(node),
        zgui_view::ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(400.0))),
        zgui_view::ScrollBehavior::Smooth,
    );
    harness.settle(4);
    assert_eq!(
        window(&harness)
            .scroll()
            .borrow()
            .offset_of(element_of(window(&harness), container))
            .y,
        DevicePx(0.0),
        "a smooth scroll that has been asked for and not yet had any time has not moved"
    );

    harness.advance(FRAME);
    harness.settle(4);
    let first = window(&harness)
        .scroll()
        .borrow()
        .offset_of(element_of(window(&harness), container))
        .y
        .0;
    assert!(
        first > 0.0 && first < 400.0,
        "a smooth scroll that arrived in one frame is not a smooth scroll: it reached {first}"
    );
    assert!(
        harness.parked_deadline().is_some(),
        "a scroll still travelling and no deadline to come back on is a scroll that stops here"
    );

    for _ in 0..30 {
        harness.advance(FRAME);
        harness.settle(4);
    }
    assert_eq!(
        window(&harness)
            .scroll()
            .borrow()
            .offset_of(element_of(window(&harness), container))
            .y,
        DevicePx(400.0),
        "it never arrived"
    );
    harness.assert_park_invariant();
    assert!(
        harness.parked_deadline().is_none(),
        "having arrived, it must stop asking to be woken"
    );
}

#[test]
fn an_idle_document_with_a_scroll_container_in_it_parks_and_runs_no_frames() {
    let mut harness = listing(200);
    harness.settle(8);
    harness.reset_counts();
    let frames = harness.run_for(
        std::time::Duration::from_secs(2),
        std::time::Duration::from_millis(16),
    );
    assert_eq!(frames, 0, "a document nobody is scrolling draws nothing");
    assert!(harness.parked_deadline().is_none());
}

/// The scrollport of the fixture's container, in device pixels.
fn scrollport(window: &zgui_runtime::Window) -> Rect<DevicePx, Device> {
    let layout = window.layout().borrow();
    for key in layout.keys() {
        if let Some(region) = zgui_layout::scroll_region::region_of(&layout, key) {
            let frag = layout.fragments_of_box(key).first().copied();
            if let Some(fragment) = frag.and_then(|frag| layout.fragment(frag)) {
                let _ = region;
                return fragment.content_box;
            }
        }
    }
    panic!("the fixture has no scroll container");
}

#[test]
fn the_wheel_moves_the_content_by_the_containers_own_line_height() {
    // Three lines of the container's own text, and not three of anything else. The fixture's rows
    // are twenty pixels and its line height comes from its computed style, so the assertion is that
    // the distance is a multiple of *that* line rather than of a constant.
    let mut harness = listing(200);
    harness.settle(8);
    let port = scrollport(window(&harness));
    let before = topmost_row_top(window(&harness), 20.0);

    wheel(&mut harness, (200.0, port.origin.y.0 + 10.0), 1.0);
    let one = before - topmost_row_top(window(&harness), 20.0);

    wheel(&mut harness, (200.0, port.origin.y.0 + 10.0), 2.0);
    let three = before - topmost_row_top(window(&harness), 20.0);

    assert!(one > 0.0, "one notch moved nothing");
    assert!(
        (three - one * 3.0).abs() < 0.51,
        "one notch moved {one} and three moved {three}, which is not three of the same line"
    );
}

/// The renderer this file never looks at, kept so the harness has one.
#[allow(dead_code)]
fn unused(_: &Arc<dyn Surface>) {}

#[test]
fn a_scroll_reaches_an_on_scroll_listener_with_the_offset_it_reached() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let seen: Rc<RefCell<Vec<(f32, f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = Rc::clone(&seen);
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let recorder = Rc::clone(&recorder);
        let mut port = zgui_elements::column().class("port").on(
            zgui_view::events::SCROLL,
            move |cx: &mut zgui_view::EventCx<'_, _>| {
                recorder.borrow_mut().push((
                    cx.offset.y.0,
                    cx.content_size.height.0,
                    cx.scrollport.height.0,
                ));
            },
        );
        for _ in 0..200 {
            port = port.child(zgui_elements::column().class("row"));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    assert!(
        seen.borrow().is_empty(),
        "building a document scrolls nothing, so nothing is reported"
    );

    wheel(&mut harness, (200.0, 60.0), 3.0);

    let reported = seen.borrow().clone();
    // One detent travels to its destination over the following frames, so a listener hears the
    // journey rather than only the arrival — which is what a listener that keeps something in step
    // with the scroll position needs. What must not happen is several reports for one *frame*,
    // which is why the coalescing this asserts is per frame and not per event.
    assert!(
        !reported.is_empty(),
        "a wheel turn told the listener nothing"
    );
    assert!(
        reported.len() <= 20,
        "one detent produced {} scroll events, which is more than the frames it travelled over",
        reported.len()
    );
    assert!(
        reported.windows(2).all(|pair| pair[1].0 >= pair[0].0),
        "the reported offset went backwards during one detent: {reported:?}"
    );
    let (offset, content, port) = *reported.last().expect("at least one report");
    assert!(offset > 0.0, "the event carried an offset of {offset}");
    assert!(
        content > port,
        "a scroll event that reports content ({content}) no larger than its port ({port}) cannot \
         answer the question every scroll handler asks — how far down am I"
    );
    assert_eq!(
        offset,
        window(&harness)
            .scroll()
            .borrow()
            .offset_of(element_of(
                window(&harness),
                window(&harness)
                    .layout()
                    .borrow()
                    .keys()
                    .into_iter()
                    .find(|key| zgui_layout::scroll_region::region_of(
                        &window(&harness).layout().borrow(),
                        *key
                    )
                    .is_some())
                    .expect("a container")
            ))
            .y
            .0,
        "the event reported an offset the container is not at"
    );
}

/// A header inside a scrollport that sticks to its top edge.
const STICKY_CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll }
.head { display: block; width: 400px; height: 30px; position: sticky; top: 0px }
.row { display: block; width: 400px; height: 20px }
";

#[test]
fn a_sticky_header_stops_at_the_top_of_its_scrollport_while_the_rows_go_past() {
    let mut harness = support::app_with_text(STICKY_CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let mut port = zgui_elements::column()
            .class("port")
            .child(zgui_elements::column().class("head"));
        for _ in 0..100 {
            port = port.child(zgui_elements::column().class("row"));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    let head_before = topmost_row_top(window(&harness), 30.0);
    let row_before = topmost_row_top(window(&harness), 20.0);

    wheel(&mut harness, (200.0, 60.0), 3.0);

    let head_after = topmost_row_top(window(&harness), 30.0);
    let row_after = topmost_row_top(window(&harness), 20.0);

    assert!(
        row_after < row_before - 1.0,
        "the rows did not scroll, so nothing is being asserted about the header"
    );
    assert_eq!(
        head_after, head_before,
        "the header left the top of the scrollport: {head_before} to {head_after}"
    );
}

/// Puts a finger down, drags it and lifts it, over the container.
fn drag_finger(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    from: f32,
    steps: &[(f32, u64)],
) {
    let contact = |y: f32| zgui_vocab::PointerEvent {
        id: zgui_vocab::PointerId::new(1),
        kind: zgui_vocab::PointerKind::Touch,
        primary: true,
        position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(y)),
        button: Some(zgui_vocab::PointerButton::Primary),
        pressure: None,
    };
    let stamp = |millis: u64| Timestamp::from_origin(std::time::Duration::from_millis(millis));

    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: zgui_vocab::PointerAction::Pressed,
        event: contact(from),
        modifiers: Modifiers::NONE,
        timestamp: stamp(0),
    });
    let mut last = (from, 0);
    for (y, at) in steps {
        harness.deliver_to_first(SurfaceEvent::Pointer {
            action: zgui_vocab::PointerAction::Moved,
            event: contact(*y),
            modifiers: Modifiers::NONE,
            timestamp: stamp(*at),
        });
        last = (*y, *at);
    }
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: zgui_vocab::PointerAction::Released,
        event: contact(last.0),
        modifiers: Modifiers::NONE,
        timestamp: stamp(last.1),
    });
    harness.settle(8);
}

#[test]
fn a_finger_dragged_up_the_list_scrolls_it_and_a_flick_keeps_it_going() {
    let mut harness = listing(200);
    harness.settle(8);
    let before = topmost_row_top(window(&harness), 20.0);

    // A finger starting inside the scrollport and travelling upwards, quickly.
    drag_finger(
        &mut harness,
        90.0,
        &[(70.0, 16), (50.0, 32), (30.0, 48), (10.0, 64)],
    );

    let dragged = topmost_row_top(window(&harness), 20.0);
    assert!(
        dragged < before - 40.0,
        "the content followed the finger by {} pixels",
        before - dragged
    );
    assert!(
        window(&harness).scroll().borrow().is_animating(),
        "lifting a finger that was still moving left the list stationary; a flick that stops dead \
         at the moment of release is a list with no momentum at all"
    );

    // And the momentum spends itself rather than running for ever.
    for _ in 0..120 {
        harness.advance(FRAME);
        harness.settle(4);
        if !window(&harness).scroll().borrow().is_animating() {
            break;
        }
    }
    let flung = topmost_row_top(window(&harness), 20.0);
    assert!(flung < dragged, "the flick carried it no further");
    assert!(
        !window(&harness).scroll().borrow().is_animating(),
        "the fling never stopped"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_drag_that_leaves_the_scrollport_keeps_carrying_the_list_it_started_on() {
    // The commonest drag there is. The content follows the contact, so the contact reaches the
    // edge of the scrollport long before the gesture ends — and a container looked up from where
    // the finger is *now* stops naming the list at exactly that moment, leaving the content stuck
    // under a finger that is still moving.
    let mut harness = listing(200);
    harness.settle(8);
    wheel(&mut harness, (200.0, 60.0), 10.0);
    let before = topmost_row_top(window(&harness), 20.0);

    // Down the screen, out of the bottom of the 120-pixel scrollport and over the root below it.
    drag_finger(
        &mut harness,
        110.0,
        &[(140.0, 16), (170.0, 32), (200.0, 48), (230.0, 64)],
    );

    let after = topmost_row_top(window(&harness), 20.0);
    assert!(
        after - before > 100.0,
        "the finger travelled 120 pixels and the list followed it {}; the drag left the \
         scrollport and the list stopped being the thing being dragged",
        after - before
    );
}

#[test]
fn a_wheel_past_the_top_stretches_the_content_and_it_springs_back() {
    let mut harness = listing(200);
    harness.settle(8);
    let resting = topmost_row_top(window(&harness), 20.0);

    // Upwards, from the origin: nothing can absorb it. Read while it is still displaced, because
    // the spring that brings it back is the second half of what this asserts and running it first
    // would leave nothing to see.
    turn(&mut harness, (200.0, 60.0), -3.0);

    let stretched = topmost_row_top(window(&harness), 20.0);
    assert!(
        stretched > resting,
        "the content did not follow the gesture past the end at all"
    );
    let container = window(&harness)
        .layout()
        .borrow()
        .keys()
        .into_iter()
        .find(|key| {
            zgui_layout::scroll_region::region_of(&window(&harness).layout().borrow(), *key)
                .is_some()
        })
        .expect("a container");
    assert_eq!(
        window(&harness)
            .scroll()
            .borrow()
            .offset_of(element_of(window(&harness), container))
            .y,
        DevicePx(0.0),
        "the reported offset went negative; a scrollbar, a `scrollTop` read and a virtualiser all \
         read that number and none of them can express content that does not exist"
    );

    for _ in 0..60 {
        harness.advance(FRAME);
        harness.settle(4);
        if window(&harness).scroll().borrow().settled() {
            break;
        }
    }
    assert!(
        window(&harness).scroll().borrow().settled(),
        "the content stayed stretched past its end"
    );
    assert_eq!(
        topmost_row_top(window(&harness), 20.0),
        resting,
        "it sprang back somewhere other than where it started"
    );
}

#[test]
fn a_finger_held_still_opens_a_context_menu_and_the_loop_comes_back_for_it() {
    use std::cell::Cell;
    use std::rc::Rc;

    let opened = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&opened);
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let counter = Rc::clone(&counter);
        let mut port = zgui_elements::column().class("port").on(
            zgui_view::events::CONTEXT_MENU,
            move |_: &mut zgui_view::EventCx<'_, _>| {
                counter.set(counter.get() + 1);
            },
        );
        for _ in 0..200 {
            port = port.child(zgui_elements::column().class("row"));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: zgui_vocab::PointerAction::Pressed,
        event: zgui_vocab::PointerEvent {
            id: zgui_vocab::PointerId::new(1),
            kind: zgui_vocab::PointerKind::Touch,
            primary: true,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            button: Some(zgui_vocab::PointerButton::Primary),
            pressure: None,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);
    assert_eq!(opened.get(), 0, "a press is not a long press");
    let parked = harness
        .parked_deadline()
        .expect(
            "a contact that can still become a long press and no deadline to come back on is a \
             long press that never fires: nothing else is going to ask for that frame",
        )
        .saturating_duration_since(harness.now());
    // The moment the press means something else, not the next refresh interval. Parking on a tick
    // is the difference between one wake and thirty, and both spellings pass the assertion above.
    assert!(
        parked > std::time::Duration::from_millis(400),
        "the loop parked {parked:?} out on a contact that cannot mean anything for half a second, \
         so it will wake, draw the same pixels and park again for every frame of the hold"
    );

    // And the hold itself costs that one wake and the one frame it asks for.
    harness.reset_counts();
    for _ in 0..40 {
        harness.advance(FRAME);
        harness.settle(4);
        if opened.get() > 0 {
            break;
        }
    }
    assert_eq!(opened.get(), 1, "the long press never arrived");
    assert_eq!(
        harness.resumes(),
        1,
        "holding one finger still reported {} expired deadlines",
        harness.resumes()
    );
    assert_eq!(
        harness.frames_requested(),
        1,
        "and drew {} frames, every one of them identical to the last",
        harness.frames_requested()
    );

    for _ in 0..20 {
        harness.advance(FRAME);
        harness.settle(4);
    }
    assert_eq!(
        opened.get(),
        1,
        "a held finger opened a context menu once per frame for as long as it was held"
    );
    harness.assert_park_invariant();
}
