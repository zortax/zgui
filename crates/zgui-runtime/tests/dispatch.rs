//! What a listener runs inside.
//!
//! A handler is arbitrary application code, and the first thing an application does in one is
//! create something reactive: a signal for the row it has just added, a stored value, a context.
//! Every one of those belongs to whichever scope was current when it was created, and with none
//! current they are discarded and leaked with nothing said — no panic, no log, and no observable
//! difference until a long-lived window has run out of memory.
//!
//! A listener runs inside the frame that dispatched it, and a frame runs inside its window's own
//! scope, so what a handler creates is owned by the window and freed when the window closes. That
//! is a property of two things being nested and nothing announces it, which is why it is asserted
//! here rather than assumed: nothing else in the suite would notice it stopping being true.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// The sheet the fixture is styled by: one element covering the whole window.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 400px; height: 300px }";

/// A press and a release over the middle of the window, which is a click on the only element in it.
fn click_at(action: PointerAction) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(150.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

#[test]
fn what_a_listener_creates_belongs_to_the_window_and_is_disposed_of_with_it() {
    // Set from a cleanup registered *inside the handler*. With no owner current when the handler
    // runs, the registration is dropped on the floor and this stays false for ever.
    let cleaned = Rc::new(Cell::new(false));
    let clicks = Rc::new(Cell::new(0_u32));

    let recorded = Rc::clone(&cleaned);
    let counted = Rc::clone(&clicks);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let recorded = Rc::clone(&recorded);
        let counted = Rc::clone(&counted);
        Box::new(
            zgui_elements::column()
                .class("root")
                .on(zgui_view::events::CLICK, move |_| {
                    counted.set(counted.get() + 1);
                    let recorded = Rc::clone(&recorded);
                    zgui_reactive::on_cleanup_local(move || recorded.set(true));
                })
                .into_view()
                .build(cx),
        )
    });

    app.settle(4);
    app.deliver_to_first(click_at(PointerAction::Moved));
    app.deliver_to_first(click_at(PointerAction::Pressed));
    app.deliver_to_first(click_at(PointerAction::Released));
    app.settle(4);
    assert_eq!(clicks.get(), 1, "the click reached the listener at all");
    assert!(
        !cleaned.get(),
        "the window is still open, so nothing it owns has been disposed of yet"
    );

    app.shut_down();
    drop(app);
    assert!(
        cleaned.get(),
        "what the handler created was owned by nothing, so closing the window freed nothing"
    );
}
