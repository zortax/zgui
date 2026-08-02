//! What a scroll from a device *means*, over a document that really scrolls.
//!
//! Everything here is asserted through the whole loop — a wheel event delivered at the platform
//! seam, dispatched, defaulted, normalised against the scrolled container's own line height, and
//! read back off the offsets the fragment pass composes with. That is the only arrangement in which
//! the claims are about scrolling rather than about arithmetic: a test that multiplied a delta by a
//! setting and compared the product would pass whatever the document did.
//!
//! The desktop's own answers reach the document through the platform, so the headless backend is
//! configured per case rather than assumed. A framework that could not be told "this desktop means
//! five lines by a detent" or "this backend has not applied the direction preference" could not
//! have those behaviours tested at all, whatever it did with them.

mod support;

use std::time::Duration;

use zgui_geom::{Css, CssPx, Point, Size};
use zgui_platform::SurfaceEvent;
use zgui_platform::scroll::{Elastic, ScrollDirection, ScrollSettings, WheelMotion};
use zgui_platform_headless::Headless;
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// One frame at the surface's refresh rate, rounded up past the deadline the park installs.
const FRAME: Duration = Duration::from_millis(20);

/// A list of twenty-pixel rows in a scrollport that shows six of them, with a known line height.
///
/// The line height is stated rather than inherited, because how far a detent travels is the count
/// of lines the desktop means times the height of a line *here*, and a test that did not fix the
/// second could not make a claim about the first.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll; line-height: 20px }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
";

/// A window over `platform` holding one scroll container with two hundred rows in it.
fn listing(platform: Headless) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text_on(platform, CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let mut port = zgui_elements::column().class("port");
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
    })
}

/// A window over a desktop that means what an ordinary one means.
fn ordinary() -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    listing(Headless::new())
}

/// A window over a desktop that answers `settings`.
fn desktop(settings: ScrollSettings) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    listing(Headless::new().with_scroll_settings(settings))
}

/// Delivers one scroll of `delta` in `phase`, over the middle of the scrollport.
fn scroll(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    delta: ScrollDelta,
    phase: ScrollPhase,
) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta,
            phase,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);
}

/// One continuous gesture that pushes past an end: a precision surface's pixels, still in motion.
fn gesture(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, by: f32) {
    scroll(
        harness,
        ScrollDelta::Pixels(zgui_geom::Size::new(CssPx(0.0), CssPx(by))),
        ScrollPhase::Moved,
    );
}

/// One detent of a notched wheel, away from or towards the user.
fn detents(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, count: f32) {
    scroll(
        harness,
        ScrollDelta::Lines { x: 0.0, y: count },
        ScrollPhase::Discrete,
    );
}

/// Runs `frames` frames of the clock, which is what carries a motion.
fn run(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, frames: usize) {
    for _ in 0..frames {
        harness.advance(FRAME);
        harness.pump();
    }
    harness.settle(2);
}

/// The window the harness is driving.
fn window(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> &zgui_runtime::Window {
    harness.app().windows().first().expect("a window")
}

/// Where the one scroll container in the document is scrolled to, in device pixels.
fn offset(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> f32 {
    let window = window(harness);
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

#[test]
fn one_detent_travels_as_many_lines_as_this_desktop_means_by_one() {
    // Twenty-pixel lines, three lines to a detent on an ordinary desktop, at a scale of one.
    let mut harness = ordinary();
    harness.settle(8);
    detents(&mut harness, 1.0);
    run(&mut harness, 30);
    assert_eq!(
        offset(&harness),
        60.0,
        "a detent moved the list somewhere other than three of its own lines"
    );
}

#[test]
fn a_desktop_that_means_something_else_by_a_detent_is_obeyed() {
    // The same document and the same event; only the desktop's answer differs. A framework with a
    // constant here would move the list sixty pixels in both cases and no test could tell.
    let mut harness = desktop(ScrollSettings::desktop().with_lines_per_notch(5.0));
    harness.settle(8);
    detents(&mut harness, 1.0);
    run(&mut harness, 30);
    assert_eq!(offset(&harness), 100.0, "five lines of twenty pixels");
}

#[test]
fn the_direction_preference_is_applied_only_when_the_backend_says_it_has_not_been() {
    // A backend reading a device rather than a desktop reports which way the hardware turned, and
    // something has to turn that into which way the person asked to go.
    let mut inverted = desktop(ScrollSettings::desktop().with_direction(ScrollDirection::Inverted));
    inverted.settle(8);
    detents(&mut inverted, -1.0);
    run(&mut inverted, 30);
    assert_eq!(
        offset(&inverted),
        60.0,
        "the backend said the preference was still to apply and it was not"
    );

    // And the ordinary case, where the input stack applied it long before the window saw it: a
    // second flip here would override a desktop setting for every program in the framework.
    let mut ordinary = ordinary();
    ordinary.settle(8);
    detents(&mut ordinary, -1.0);
    run(&mut ordinary, 30);
    assert_eq!(
        offset(&ordinary),
        0.0,
        "a detent towards the top of a list already at the top moved it down"
    );
}

#[test]
fn a_detent_travels_to_its_destination_rather_than_arriving_at_it() {
    let mut harness = ordinary();
    harness.settle(8);
    detents(&mut harness, 1.0);

    // The frame the event is drained in is a frame of the motion, not the whole of it.
    harness.advance(FRAME);
    harness.pump();
    let after_one = offset(&harness);
    assert!(
        after_one > 0.0,
        "nothing moved in the first frame, which reads as a dropped detent"
    );
    assert!(
        after_one < 60.0,
        "the detent landed in one frame: {after_one} of 60 device pixels, which is a jump"
    );

    run(&mut harness, 30);
    assert_eq!(offset(&harness), 60.0, "and it did arrive");
}

#[test]
fn detents_in_quick_succession_compose_into_one_continuing_motion() {
    let mut harness = ordinary();
    harness.settle(8);

    // Three detents inside the time one of them takes to travel, which is an ordinary spin of a
    // wheel. Each has to add to where the motion is *heading*; a motion re-aimed from where the
    // content has got to so far throws away everything the earlier detents had not yet covered,
    // and three quick detents land barely further than one.
    detents(&mut harness, 1.0);
    harness.advance(FRAME);
    harness.pump();
    detents(&mut harness, 1.0);
    harness.advance(FRAME);
    harness.pump();
    detents(&mut harness, 1.0);
    run(&mut harness, 30);

    assert_eq!(
        offset(&harness),
        180.0,
        "three detents did not add up: a spin of the wheel goes three times as far as one click"
    );
}

#[test]
fn a_continuous_surfaces_pixels_are_not_animated_a_second_time() {
    // A trackpad's deltas are already a motion. Carrying each of them to its own destination over
    // a quarter of a second is what makes a trackpad feel like treacle, and it also makes the
    // content lag the fingers by the length of the animation.
    let mut harness = ordinary();
    harness.settle(8);
    scroll(
        &mut harness,
        ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(50.0))),
        ScrollPhase::Moved,
    );
    assert_eq!(
        offset(&harness),
        50.0,
        "a gesture's pixels did not land in the frame they arrived in"
    );

    // And a desktop that animates its own detents gets the same treatment for its wheel, because
    // there the *wheel* is the continuous stream.
    let mut smoothed = desktop(ScrollSettings::desktop().with_wheel(WheelMotion::Continuous));
    smoothed.settle(8);
    detents(&mut smoothed, 1.0);
    assert_eq!(
        offset(&smoothed),
        60.0,
        "the platform said it animates its own detents and the framework animated them again"
    );
}

#[test]
fn an_edge_pulled_past_its_end_springs_back_on_frames_rather_than_on_events() {
    let mut harness = ordinary();
    harness.settle(8);

    // Against the top, where nothing can absorb it: the whole delta becomes displacement. Pushed
    // by a gesture, because a gesture is what an edge follows — a detent is refused by default and
    // the case this is about is the spring's *clock*, not which input reaches it.
    gesture(&mut harness, -60.0);
    let held = window(&harness).scroll().borrow();
    let container = {
        let window = window(&harness);
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .find(|key| zgui_layout::scroll_region::region_of(&layout, *key).is_some())
            .and_then(|key| layout.node(key).source)
            .expect("the fixture has a scroll container")
    };
    let stretched = held.elastic_of(container).height.0;
    drop(held);
    assert!(
        stretched < 0.0,
        "the content did not follow past the top of the list at all"
    );

    // The park has to keep coming back for it. An edge that only moves when something else happens
    // to ask for a frame stays stretched for as long as the window is idle, which is for ever.
    assert!(
        harness.parked_deadline().is_some(),
        "the displaced edge asked for no frame, so nothing will ever bring it back"
    );

    // No event of any kind from here on: every step is the clock and nothing else.
    let mut positions = Vec::new();
    for _ in 0..30 {
        harness.advance(FRAME);
        harness.pump();
        positions.push(
            window(&harness)
                .scroll()
                .borrow()
                .elastic_of(container)
                .height
                .0,
        );
    }
    assert!(
        positions.windows(2).all(|pair| pair[1] >= pair[0]),
        "the edge did not return monotonically, which is what reads as a stutter: {positions:?}"
    );
    assert!(
        window(&harness).scroll().borrow().settled(),
        "the edge never came back: it is at {}",
        window(&harness)
            .scroll()
            .borrow()
            .elastic_of(container)
            .height
            .0
    );
    assert_eq!(
        offset(&harness),
        0.0,
        "the reported offset went past the top; a scrollbar and a virtualiser both read that number"
    );
}

/// Where a container is displaced past its end, for the caller that asked for it.
fn stretched(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> f32 {
    let window = window(harness);
    let container = {
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .find(|key| zgui_layout::scroll_region::region_of(&layout, *key).is_some())
            .and_then(|key| layout.node(key).source)
            .expect("the fixture has a scroll container")
    };
    let held = window.scroll().borrow();
    held.elastic_of(container).height.0
}

#[test]
fn a_wheel_stops_at_the_end_and_a_gesture_pulls_past_it() {
    // The whole of the setting, in one case each. A detent has nothing continuous behind it for an
    // edge to follow, so a spring there bounces once per click against an end the person has
    // already reached; a contact that is still moving is exactly what an edge is meant to track.
    let mut wheel = ordinary();
    wheel.settle(8);
    detents(&mut wheel, -1.0);
    assert_eq!(
        stretched(&wheel),
        0.0,
        "a notched wheel pushed past the top and the edge followed it"
    );

    let mut surface = ordinary();
    surface.settle(8);
    gesture(&mut surface, -60.0);
    assert!(
        stretched(&surface) < 0.0,
        "a gesture pushed past the top and the edge did not follow it"
    );
}

#[test]
fn a_desktop_may_ask_for_the_spring_everywhere_or_nowhere() {
    // The two answers that do not distinguish. `Always` is what somebody who wants the bounce on a
    // wheel sets; `Never` is what somebody who wants no bounce at all sets, and it has to reach the
    // gesture path too or it is not what it says.
    let mut always = desktop(ScrollSettings::desktop().with_elastic(Elastic::Always));
    always.settle(8);
    detents(&mut always, -1.0);
    assert!(
        stretched(&always) < 0.0,
        "the desktop asked for the spring on a wheel and did not get it"
    );

    let mut never = desktop(ScrollSettings::desktop().with_elastic(Elastic::Never));
    never.settle(8);
    gesture(&mut never, -60.0);
    assert_eq!(
        stretched(&never),
        0.0,
        "the desktop asked for no spring anywhere and a gesture still stretched an edge"
    );
}
