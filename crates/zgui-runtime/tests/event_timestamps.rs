//! A handler sees the moment the platform stamped an event, and the events that an input event
//! causes carry the stamp of that input event.
//!
//! The frame drains its queue later than the events happened. The stamps must therefore come from
//! the events and never from the frame, and they must not go backwards inside one batch.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, EventCx, EventType, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 400px; height: 300px }";

/// A mouse event at `x` along the middle of the window, stamped `ms` after the origin.
fn mouse(action: PointerAction, x: f32, ms: u64) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(150.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::from_origin(Duration::from_millis(ms)),
    }
}

/// One dispatch a listener saw: the event's name, its stamp and the stamps of its samples, in
/// milliseconds after the origin.
#[derive(Clone, Debug, PartialEq)]
struct Seen {
    kind: &'static str,
    at: u128,
    samples: Vec<u128>,
}

fn millis(stamp: Timestamp) -> u128 {
    stamp.since_origin().as_millis()
}

/// A listener that writes the name `kind` and the event's stamp into `log`.
fn record<E: EventType>(
    log: &Rc<RefCell<Vec<Seen>>>,
    kind: &'static str,
) -> impl Fn(&mut EventCx<'_, E>) + 'static {
    let log = Rc::clone(log);
    move |ev| {
        log.borrow_mut().push(Seen {
            kind,
            at: millis(ev.timestamp),
            samples: Vec::new(),
        });
    }
}

fn app(log: &Rc<RefCell<Vec<Seen>>>) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let log = Rc::clone(log);
    support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let moves = Rc::clone(&log);
        Box::new(
            zgui_elements::column()
                .class("root")
                .on(zgui_view::events::POINTER_ENTER, record(&log, "enter"))
                .on(zgui_view::events::POINTER_MOVE, move |ev| {
                    moves.borrow_mut().push(Seen {
                        kind: "move",
                        at: millis(ev.timestamp),
                        samples: ev.coalesced().iter().map(|s| millis(s.timestamp)).collect(),
                    });
                })
                .on(zgui_view::events::POINTER_DOWN, record(&log, "down"))
                .on(zgui_view::events::POINTER_UP, record(&log, "up"))
                .on(zgui_view::events::CLICK, record(&log, "click"))
                .into_view()
                .build(cx),
        )
    })
}

fn seen(kind: &'static str, at: u128, samples: &[u128]) -> Seen {
    Seen {
        kind,
        at,
        samples: samples.to_vec(),
    }
}

#[test]
fn handlers_see_the_stamp_of_the_input_that_caused_the_event() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut app = app(&log);
    app.settle(4);
    app.advance(Duration::from_millis(100));
    app.settle(4);
    log.borrow_mut().clear();

    app.deliver_to_first(mouse(PointerAction::Moved, 10.0, 10));
    app.deliver_to_first(mouse(PointerAction::Moved, 12.0, 12));
    app.deliver_to_first(mouse(PointerAction::Pressed, 12.0, 20));
    app.deliver_to_first(mouse(PointerAction::Released, 12.0, 30));
    app.deliver_to_first(mouse(PointerAction::Moved, 14.0, 40));
    app.settle(4);

    let log = log.borrow();
    assert_eq!(
        *log,
        vec![
            seen("enter", 12, &[]),
            seen("move", 12, &[10]),
            seen("down", 20, &[]),
            seen("up", 30, &[]),
            seen("click", 30, &[]),
            seen("move", 40, &[]),
        ],
        "each event carries its own stamp, and an enter or a click carries its cause's stamp"
    );
    assert!(
        log.windows(2).all(|pair| pair[0].at <= pair[1].at),
        "the stamps do not go backwards: {log:?}"
    );
    drop(log);
    app.shut_down();
}
