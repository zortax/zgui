//! Moves that arrive between two frames are delivered as one.
//!
//! A pointer reports its position far more often than a frame can answer, and every move that is
//! routed costs a hit test, a listener run and a settle of what the listener wrote. So a move
//! queued behind a move of the same pointer takes its place, and the positions it stood for ride
//! along as samples. A press, a release or a move of another pointer is a barrier: nothing is
//! delivered out of order, and nothing but a move is ever folded.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_profile::{Counter, counter};
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, PointerId, PointerKind, Timestamp};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 400px; height: 300px }";

/// A mouse event at `x` along the middle of the window.
fn mouse(action: PointerAction, x: f32) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(150.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// A move of a second pointer, so the fold has something it must not merge across.
fn other_pointer_move(x: f32) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent {
            id: PointerId::new(7),
            kind: PointerKind::Touch,
            primary: false,
            position: Point::new(CssPx(x), CssPx(150.0)),
            button: None,
            pressure: None,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// What a listener on the only element saw: one entry per dispatch, with the folded samples.
#[derive(Clone, Debug, PartialEq)]
enum Seen {
    Move { x: f32, samples: Vec<f32> },
    Down,
}

fn app(log: &Rc<RefCell<Vec<Seen>>>) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let moves = Rc::clone(log);
    let downs = Rc::clone(log);
    support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let moves = Rc::clone(&moves);
        let downs = Rc::clone(&downs);
        Box::new(
            zgui_elements::column()
                .class("root")
                .on(zgui_view::events::POINTER_MOVE, move |ev| {
                    moves.borrow_mut().push(Seen::Move {
                        x: ev.position.x.0,
                        samples: ev.coalesced().iter().map(|s| s.position.x.0).collect(),
                    });
                })
                .on(zgui_view::events::POINTER_DOWN, move |_| {
                    downs.borrow_mut().push(Seen::Down);
                })
                .into_view()
                .build(cx),
        )
    })
}

#[test]
fn moves_queued_together_are_delivered_as_the_last_with_the_rest_as_samples() {
    let _guard = counter::exclusive();
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut app = app(&log);
    app.settle(4);
    log.borrow_mut().clear();

    counter::reset();
    for x in [10.0, 20.0, 30.0, 40.0, 50.0] {
        app.deliver_to_first(mouse(PointerAction::Moved, x));
    }
    app.settle(4);

    assert_eq!(
        *log.borrow(),
        vec![Seen::Move {
            x: 50.0,
            samples: vec![10.0, 20.0, 30.0, 40.0],
        }],
        "five moves became one dispatch at the last position, carrying the four before it"
    );
    assert_eq!(
        counter::snapshot().get(Counter::PointerMovesCoalesced),
        4,
        "four moves were folded"
    );
    app.shut_down();
}

#[test]
fn a_press_between_moves_is_a_barrier() {
    let _guard = counter::exclusive();
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut app = app(&log);
    app.settle(4);
    log.borrow_mut().clear();

    app.deliver_to_first(mouse(PointerAction::Moved, 10.0));
    app.deliver_to_first(mouse(PointerAction::Pressed, 10.0));
    app.deliver_to_first(mouse(PointerAction::Moved, 20.0));
    app.deliver_to_first(mouse(PointerAction::Moved, 30.0));
    app.settle(4);

    assert_eq!(
        *log.borrow(),
        vec![
            Seen::Move {
                x: 10.0,
                samples: vec![],
            },
            Seen::Down,
            Seen::Move {
                x: 30.0,
                samples: vec![20.0],
            },
        ],
        "the move before the press and the press itself were delivered in order"
    );
    app.shut_down();
}

#[test]
fn moves_of_different_pointers_are_not_folded_together() {
    let _guard = counter::exclusive();
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut app = app(&log);
    app.settle(4);
    log.borrow_mut().clear();

    app.deliver_to_first(mouse(PointerAction::Moved, 10.0));
    app.deliver_to_first(other_pointer_move(200.0));
    app.deliver_to_first(mouse(PointerAction::Moved, 20.0));
    app.settle(4);

    let seen = log.borrow();
    assert_eq!(
        seen.len(),
        3,
        "three pointers' worth of moves, none folded: {seen:?}"
    );
    assert!(
        seen.iter()
            .all(|s| matches!(s, Seen::Move { samples, .. } if samples.is_empty())),
        "nothing carried a sample: {seen:?}"
    );
    app.shut_down();
}
