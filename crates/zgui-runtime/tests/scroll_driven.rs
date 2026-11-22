//! Scrolling the compositor drives: gestures go to the compositor, and the window runs no frame of
//! its own while the content moves over what it recorded.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{Css, CssPx, Point, Size};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_profile::Counter;
use zgui_runtime::Runtime;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{
    Modifiers, PointerAction, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase,
    Timestamp, WheelEvent,
};

const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
.gap { display: block; width: 400px; height: 2000px }
";

/// The refresh interval the headless platform paces frames at.
const TICK: Duration = Duration::from_millis(8);

/// The rows each click lands on, by index.
type Clicked = Rc<RefCell<Vec<usize>>>;

/// A port of `rows` rows with click listeners, followed by a blank gap when `gap` is set.
fn listing(rows: usize, gap: bool, clicked: &Clicked) -> Harness<Runtime> {
    listing_in(CSS, rows, gap, clicked)
}

/// The same under the style sheet `css`.
fn listing_in(css: &str, rows: usize, gap: bool, clicked: &Clicked) -> Harness<Runtime> {
    let clicked = Rc::clone(clicked);
    support::app_with_text(css, move |cx: &mut BuildCx<'_>| {
        let mut port = zgui_elements::column().class("port");
        for index in 0..rows {
            let reached = Rc::clone(&clicked);
            port = port.child(
                zgui_elements::column()
                    .class("row")
                    .on(zgui_view::events::CLICK, move |_| {
                        reached.borrow_mut().push(index)
                    }),
            );
        }
        if gap {
            port = port.child(zgui_elements::column().class("gap"));
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

/// Delivers one touchpad step of `dy` CSS pixels in `phase` over the port.
fn touch(harness: &mut Harness<Runtime>, dy: f32, phase: ScrollPhase) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(dy))),
            phase,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
}

/// Drags the port by six steps of `dy` CSS pixels a tick apart and lifts the fingers, so the
/// compositor flings it.
fn fling(harness: &mut Harness<Runtime>, dy: f32) {
    touch(harness, 0.0, ScrollPhase::Started);
    harness.settle(4);
    for _ in 0..6 {
        harness.advance(TICK);
        touch(harness, dy, ScrollPhase::Moved);
        harness.pump();
    }
    touch(harness, 0.0, ScrollPhase::Ended);
    harness.pump();
}

/// Runs `ticks` refresh intervals.
fn run(harness: &mut Harness<Runtime>, ticks: usize) {
    for _ in 0..ticks {
        harness.advance(TICK);
        harness.pump();
    }
}

/// Where the port is scrolled to, in device pixels, as the window's own layout reads it.
fn offset(harness: &Harness<Runtime>) -> f32 {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    let scroll = window.scroll().borrow();
    layout
        .keys()
        .into_iter()
        .filter(|key| zgui_layout::scroll_region::region_of(&layout, *key).is_some())
        .filter_map(|key| layout.node(key).source)
        .map(|element| scroll.offset_of(element).y.0)
        .fold(0.0, f32::max)
}

/// Moves the pointer over the port, and lets the frames that asks for run.
fn hover_port(harness: &mut Harness<Runtime>) {
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(60.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);
}

/// Clicks at `y` CSS pixels, with no frame before the press.
fn click(harness: &mut Harness<Runtime>, y: f32) {
    for action in [PointerAction::Pressed, PointerAction::Released] {
        harness.deliver_to_first(SurfaceEvent::Pointer {
            action,
            event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(y))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }
    harness.pump();
}

#[test]
fn a_fling_over_recorded_rows_runs_no_frame_of_the_window_until_it_rests() {
    let _turn = zgui_profile::counter::exclusive();
    let clicked = Clicked::default();
    let mut harness = listing(10, false, &clicked);
    harness.settle(8);

    fling(&mut harness, 8.0);
    let before = zgui_profile::counter::snapshot();
    run(&mut harness, 120);
    let moved = before.delta(&zgui_profile::counter::snapshot());

    assert!(
        moved.get(Counter::CompositorTicks) >= 10,
        "the fling presented {} frames of the compositor's",
        moved.get(Counter::CompositorTicks)
    );
    assert_eq!(
        moved.get(Counter::ZguiFrames),
        1,
        "the window ran frames during the fling, beyond the one that adopts where it rests"
    );
    assert!(offset(&harness) > 0.0, "the fling moved nothing");
}

/// Flings a port of 40 rows under the style sheet `css`, with the pointer over the port when
/// `hovering` is set, and returns how many frames the window ran and how many rows passed under
/// the pointer.
fn fling_under(css: &str, hovering: bool) -> (u64, u64) {
    let clicked = Clicked::default();
    let mut harness = listing_in(css, 40, false, &clicked);
    harness.settle(8);
    if hovering {
        hover_port(&mut harness);
    }

    fling(&mut harness, 8.0);
    let from = offset(&harness);
    let before = zgui_profile::counter::snapshot();
    run(&mut harness, 120);
    let moved = before.delta(&zgui_profile::counter::snapshot());

    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rows = ((offset(&harness) - from) / 20.0).ceil() as u64 + 1;
    (moved.get(Counter::ZguiFrames), rows)
}

#[test]
fn a_fling_under_the_pointer_over_rows_no_hover_rule_matches_runs_no_frame_for_the_hover() {
    let _turn = zgui_profile::counter::exclusive();
    let (away, _) = fling_under(CSS, false);
    let (over, rows) = fling_under(CSS, true);

    assert!(
        rows > 3,
        "the fling moved {rows} rows under the pointer, too few to tell"
    );
    assert_eq!(
        over, away,
        "the window ran {over} frames with the pointer over rows no :hover rule matches, and \
         {away} with the pointer away"
    );
}

#[test]
fn a_fling_under_the_pointer_wakes_the_window_at_most_once_per_row_a_hover_rule_matches() {
    let _turn = zgui_profile::counter::exclusive();
    let css = format!("{CSS} .row:hover {{ background-color: #404040 }}");
    let (away, _) = fling_under(&css, false);
    let (over, rows) = fling_under(&css, true);

    assert!(
        rows > 3,
        "the fling moved {rows} rows under the pointer, too few to tell"
    );
    assert!(
        over > away,
        "the window ran {over} frames with the pointer over the rows and {away} with it away, \
         so the hover never followed the rows"
    );
    assert!(
        over - away <= rows,
        "the hover woke the window {} times while {rows} rows passed under the pointer",
        over - away
    );
}

#[test]
fn a_fling_past_the_painted_rows_wakes_the_window_before_they_end() {
    let _turn = zgui_profile::counter::exclusive();
    let clicked = Clicked::default();
    let mut harness = listing(12, true, &clicked);
    harness.settle(8);

    fling(&mut harness, 20.0);
    let before = zgui_profile::counter::snapshot();
    let mut woken = 0;
    for _ in 0..120 {
        let frames = zgui_profile::counter::snapshot().get(Counter::ZguiFrames);
        harness.advance(TICK);
        harness.pump();
        let shown = offset(&harness);
        if zgui_profile::counter::snapshot().get(Counter::ZguiFrames) > frames && shown < 240.0 {
            woken += 1;
        }
    }
    let moved = before.delta(&zgui_profile::counter::snapshot());

    assert!(
        offset(&harness) > 240.0,
        "the fling stopped at {} before the rows ended, so it never reached the gap",
        offset(&harness)
    );
    assert!(
        woken >= 1,
        "the window ran no frame while the rows ended under the port, in {} frames",
        moved.get(Counter::ZguiFrames)
    );
}

#[test]
fn a_click_during_a_fling_lands_on_the_row_the_compositor_shows() {
    let _turn = zgui_profile::counter::exclusive();
    let clicked = Clicked::default();
    let mut harness = listing(60, false, &clicked);
    harness.settle(8);

    fling(&mut harness, 20.0);
    run(&mut harness, 3);
    let adopted = offset(&harness);
    click(&mut harness, 50.0);
    let shown = offset(&harness);

    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let row = |offset: f32| ((50.0 + offset) / 20.0).floor() as usize;
    assert_ne!(
        row(shown),
        row(adopted),
        "the compositor moved the list from {adopted} to {shown}, and the same row is under the \
         click at both, so the click tells nothing"
    );
    assert_eq!(*clicked.borrow(), vec![row(shown)], "the click at {shown}");
}

#[test]
fn a_freeze_of_the_root_stops_a_fling_of_the_root() {
    let _turn = zgui_profile::counter::exclusive();
    let css = "root { display: block; width: 400px; height: 300px; overflow-y: scroll }
               .row { display: block; height: 20px; background-color: #202020 }";
    let mut harness = support::app_with_text(css, move |cx: &mut BuildCx<'_>| {
        let mut page = zgui_elements::column();
        for _ in 0..200 {
            page = page.child(zgui_elements::column().class("row"));
        }
        Box::new(page.into_view().build(cx))
    });
    harness.settle(8);

    fling(&mut harness, 20.0);
    run(&mut harness, 3);
    let host = harness.app().windows()[0].host().clone();
    zgui_view::ViewHost::freeze_scrolling(&*host, true);
    run(&mut harness, 2);
    let frozen = offset(&harness);
    run(&mut harness, 60);

    assert!(frozen > 0.0, "the fling moved nothing before the freeze");
    assert_eq!(
        offset(&harness),
        frozen,
        "the fling went on from {frozen} after the root froze"
    );
}

#[test]
fn a_reader_finds_the_rows_where_a_fling_left_them() {
    use zgui_view::A11yBinding;
    use zgui_vocab::Role;
    let _turn = zgui_profile::counter::exclusive();
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let mut port = zgui_elements::column().class("port");
        for index in 0..60 {
            port = port.child(
                zgui_elements::column()
                    .class("row")
                    .a11y(A11yBinding::new(Role::Button).label(format!("Row {index}"))),
            );
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

    fling(&mut harness, 20.0);
    run(&mut harness, 240);
    let rested = offset(&harness);
    assert!(rested > 100.0, "the fling moved the list {rested} pixels");

    let tree = support::a11y_tree(&harness);
    let state = tree.state();
    let top = |label: &str| {
        let mut found = None;
        let mut stack = vec![state.root()];
        while let Some(node) = stack.pop() {
            if node.label().as_deref() == Some(label) {
                found = node.bounding_box().map(|drawn| drawn.y0);
            }
            stack.extend(node.filtered_children(&accesskit_consumer::common_filter));
        }
        found
    };
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let first = (rested / 20.0).ceil() as usize;
    assert_eq!(
        top(&format!("Row {first}")),
        Some(f64::from(first as f32 * 20.0 - rested)),
        "the reader reads the row at {rested}"
    );
}

#[test]
fn a_sticky_header_stays_at_the_top_of_the_port_through_a_fling() {
    const STICKY: &str = "
        root { display: block; width: 400px; height: 300px }
        .port { display: block; width: 400px; height: 120px; overflow: scroll }
        .head { display: block; width: 400px; height: 30px; position: sticky; top: 0px;
                background-color: #404040 }
        .row { display: block; width: 400px; height: 20px; background-color: #202020 }
    ";
    let _turn = zgui_profile::counter::exclusive();
    let mut harness = support::app_with_text(STICKY, move |cx: &mut BuildCx<'_>| {
        let mut port = zgui_elements::column()
            .class("port")
            .child(zgui_elements::column().class("head"));
        for _ in 0..60 {
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
    let head = |harness: &Harness<Runtime>| {
        let window = harness.app().windows().first().expect("a window");
        let layout = window.layout().borrow();
        layout
            .keys()
            .iter()
            .flat_map(|key| layout.fragments_of_box(*key).to_vec())
            .filter_map(|frag| layout.fragment(frag))
            .filter(|fragment| (fragment.border_box.size.height.0 - 30.0).abs() < 0.5)
            .map(|fragment| {
                fragment
                    .on_device(fragment.border_box, &window.placements())
                    .origin
                    .y
                    .0
            })
            .next()
            .expect("the header has a fragment")
    };
    let before = head(&harness);

    let start = zgui_profile::counter::snapshot();
    fling(&mut harness, 20.0);
    run(&mut harness, 240);
    let moved = start.delta(&zgui_profile::counter::snapshot());

    assert!(
        moved.get(Counter::CompositorTicks) > 0 && offset(&harness) > 100.0,
        "the compositor did not carry the list"
    );
    assert_eq!(
        head(&harness),
        before,
        "the header left the top of the port"
    );
}
