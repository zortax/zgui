//! What a wheel and a key turn into.
//!
//! Both are aimed differently from a press — a wheel at what is under the pointer, a key at
//! whatever has focus — and both produce a framework default that the caller carries out. These
//! are the two paths a press does not exercise.

mod support;

use support::{Element, Fixture, Session};
use zgui_geom::{Css, CssPx, Device, Scale};
use zgui_input::normalize::scroll::{self, ScrollUnits};
use zgui_input::{FocusDirection, FocusSource, FrameworkDefault};
use zgui_vocab::{
    Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerId, PointerKind,
    ScrollDelta, ScrollPhase, WheelEvent,
};

/// A page with a scrolling list on it, and a button inside the list.
fn scrollable() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![Element::new("scroll").children(vec![
            Element::new("row").children(vec![Element::new("control")]),
        ])]),
        "root { display: block; width: 300px }
         scroll { display: block; width: 300px; height: 100px; overflow: scroll }
         row { display: block; height: 400px }
         control { display: block; height: 30px }",
    )
}

#[test]
fn a_wheel_scrolls_the_nearest_container_and_its_distance_is_the_targets_own_line_height() {
    // The two halves of a scroll, in the order they have to happen in. *Which* container scrolls
    // is a question about the path, and this crate answers it. *How far* three notches are is a
    // question about that container's own text, which is not known until the container is — so
    // the distance is resolved second, against the answer to the first.
    let mut session = Session::new(scrollable());
    let container = session.fixture.key("scroll");
    let point = session.fixture.centre_of("control");

    let event = WheelEvent {
        delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
        phase: ScrollPhase::Discrete,
        position: zgui_geom::Point::new(CssPx(point.x.0), CssPx(point.y.0)),
        id: PointerId::MOUSE,
        kind: PointerKind::Mouse,
    };

    let default = {
        let filter = session.fixture.filter();
        let world = session.fixture.world(&filter);
        session.router.wheel(&world, &event).default
    };

    let FrameworkDefault::Scroll {
        container: asked,
        delta,
        ..
    } = default.expect("something under the pointer scrolls")
    else {
        panic!("a wheel scrolls");
    };
    assert_eq!(
        asked, container,
        "the list scrolls, not the button the pointer is actually over"
    );

    // A notch is three lines of *this* container's text. The same event over a document whose
    // text is twice the size moves twice as far, which is why the height is an argument and not a
    // constant anywhere in this crate.
    let small = scroll::to_device(
        delta,
        ScrollUnits::for_scrollport(CssPx(16.0), CssPx(100.0)),
        Scale::<Css, Device>::new(1.0),
    );
    let large = scroll::to_device(
        delta,
        ScrollUnits::for_scrollport(CssPx(32.0), CssPx(100.0)),
        Scale::<Css, Device>::new(1.0),
    );
    assert_eq!(small.height.0, -48.0);
    assert_eq!(large.height.0, -96.0);
}

#[test]
fn a_wheel_over_nothing_that_scrolls_asks_for_no_scroll() {
    let mut session = Session::new(Fixture::new(
        Element::new("root").children(vec![Element::new("row")]),
        "root, row { display: block; width: 300px; height: 40px }",
    ));
    let point = session.fixture.centre_of("row");
    let event = WheelEvent {
        delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
        phase: ScrollPhase::Discrete,
        position: zgui_geom::Point::new(CssPx(point.x.0), CssPx(point.y.0)),
        id: PointerId::MOUSE,
        kind: PointerKind::Mouse,
    };
    let filter = session.fixture.filter();
    let world = session.fixture.world(&filter);
    assert!(session.router.wheel(&world, &event).default.is_none());
}

#[test]
fn a_key_is_aimed_at_whatever_has_focus_and_tab_asks_for_the_next_stop() {
    let mut session = Session::new(scrollable());
    let control = session.fixture.key("control");
    {
        let filter = session.fixture.filter();
        let world = session.fixture.world(&filter);
        session
            .router
            .focus(&world, Some(control), FocusSource::Keyboard);
    }

    let tab = KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab));
    let (path, default) = {
        let filter = session.fixture.filter();
        let world = session.fixture.world(&filter);
        let routed = session
            .router
            .key(&world, KeyState::Pressed, &tab, Modifiers::NONE, &[]);
        (routed.chain.path().to_vec(), routed.default)
    };

    assert_eq!(
        path.last().copied(),
        Some(control),
        "the key travels to the focused element, not to whatever the pointer is over"
    );
    assert_eq!(
        default,
        Some(FrameworkDefault::MoveFocus(FocusDirection::Next))
    );
}

#[test]
fn a_key_with_nothing_focused_still_reaches_the_root() {
    // The path is the root and nothing else, which is the whole of what makes a window shortcut a
    // registration rather than a listener: an event that stopped at the document would reach
    // nothing at all, and one that walked further would reach every key handler in the page.
    let mut session = Session::new(scrollable());
    let mut event = KeyEvent::named(NamedKey::Escape, PhysicalKey::Code(KeyCode::Escape));
    event.key = Key::Named(NamedKey::Escape);

    let filter = session.fixture.filter();
    let world = session.fixture.world(&filter);
    let routed = session
        .router
        .key(&world, KeyState::Pressed, &event, Modifiers::NONE, &[]);

    assert_eq!(routed.chain.path().len(), 1);
    assert_eq!(routed.chain.target(), Some(session.fixture.key("root")));
    assert_eq!(
        routed.default, None,
        "and escape is not one of the two keys the framework acts on by itself"
    );
}

#[test]
fn a_release_of_a_key_asks_for_nothing_however_the_press_was_read() {
    let mut session = Session::new(scrollable());
    let tab = KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab));
    let filter = session.fixture.filter();
    let world = session.fixture.world(&filter);
    let routed = session
        .router
        .key(&world, KeyState::Released, &tab, Modifiers::NONE, &[]);
    assert_eq!(routed.kind, zgui_vocab::EventKind::KeyUp);
    assert_eq!(routed.default, None);
}
