//! Pointer capture, and the re-test under a pointer that has not moved.

mod support;

use support::{Element, Fixture, Session};
use zgui_geom::{DevicePx, Point};
use zgui_vocab::{PointerAction, PointerId, UiState};

/// A track with a thumb in it, and a wide area beside them to drag onto.
fn slider() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![
            Element::new("row").children(vec![Element::new("control")]),
            Element::new("box").class("elsewhere"),
        ]),
        "root, row { display: block; width: 300px }
         control { display: block; height: 20px }
         box { display: block; height: 200px }",
    )
}

#[test]
fn a_captured_pointer_keeps_reaching_the_element_that_captured_it() {
    let mut session = Session::new(slider());
    let thumb = session.fixture.key("control");
    let on_thumb = session.fixture.centre_of("control");
    let far_away = Point::new(on_thumb.x, DevicePx(on_thumb.y.0 + 120.0));

    // Without a capture, moving off the thumb aims somewhere else entirely.
    session.pointer_at(on_thumb, PointerAction::Pressed);
    let uncaptured = session.pointer_at(far_away, PointerAction::Moved);
    assert!(
        !uncaptured.contains(&thumb),
        "the fixture is only a test of capture if the pointer really left the thumb"
    );

    // With one, every later event is aimed at the thumb wherever the pointer is.
    session.router.capture_mut().set(PointerId::MOUSE, thumb);
    let captured = session.pointer_at(far_away, PointerAction::Moved);
    assert_eq!(
        captured.last().copied(),
        Some(thumb),
        "a drag past the edge of a slider keeps moving the slider"
    );

    session.router.capture_mut().release(PointerId::MOUSE);
    let released = session.pointer_at(far_away, PointerAction::Moved);
    assert!(!released.contains(&thumb));
}

#[test]
fn an_element_that_goes_away_while_it_holds_the_pointer_releases_it() {
    let mut session = Session::new(slider());
    let thumb = session.fixture.key("control");
    session.router.capture_mut().set(PointerId::MOUSE, thumb);

    session.router.forget(thumb);
    assert_eq!(
        session.router.capture().of(PointerId::MOUSE),
        None,
        "otherwise every later event is aimed at a node that no longer exists"
    );
}

#[test]
fn a_frame_that_moves_a_box_out_from_under_a_stationary_pointer_rewrites_the_hover() {
    // No pointer event happens here at all: the geometry changed underneath one that is standing
    // still, and without the re-test the element it is now over never learns that it is hovered.
    let mut session = Session::new(Fixture::new(
        Element::new("root").children(vec![
            Element::new("row").class("first"),
            Element::new("row").class("second"),
        ]),
        "root { display: block; width: 300px }
         row { display: block; height: 40px }
         .tall { height: 200px }",
    ));

    let point = Point::new(DevicePx(10.0), DevicePx(60.0));
    session.pointer_at(point, PointerAction::Moved);
    // `find` and `key` answer with the first element of a name in document order, so this is the
    // upper row — the one the pointer is *not* over, and the one that is about to grow.
    let upper_index = session.fixture.find("row");
    let upper = session.fixture.key("row");
    let hovered_first = session.router.interaction().hover.target();
    assert!(hovered_first.is_some());
    assert_ne!(
        hovered_first,
        Some(upper),
        "the pointer starts over the lower row"
    );

    // The upper row grows, so the point that was over the lower row is now over the upper one.
    session
        .fixture
        .document
        .edit(&zgui_dom::EverythingMatters, |edit| {
            edit.add_class(upper_index, zgui_interned::ClassName::new("tall"));
        })
        .expect("not poisoned");
    session.fixture.restyle();
    session.fixture.lay_out();

    let moved = {
        let filter = session.fixture.filter();
        let world = session.fixture.world(&filter);
        session.router.rehit(&world)
    };
    assert!(!moved.is_empty(), "the re-test wrote something");
    assert_eq!(
        session.router.interaction().hover.target(),
        Some(upper),
        "and what is under the stationary pointer is hovered now"
    );

    let index = session
        .fixture
        .document
        .store()
        .index_of(upper)
        .expect("a live element");
    assert!(
        session
            .fixture
            .document
            .store()
            .core(index)
            .ui_state()
            .contains(UiState::HOVER)
    );
}
