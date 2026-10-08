//! Two clicks of the pointer on one element, soon and near, are a double click.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// The sheet the fixture is styled by: one element covering the whole window.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 400px; height: 300px }";

/// One pointer action at `x`, 150, at `at` after the start.
fn pointer(action: PointerAction, x: f32, at: Duration) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(150.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN + at,
    }
}

/// A window whose one element counts its clicks and its double clicks.
fn counting() -> (
    zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    Rc<Cell<u32>>,
    Rc<Cell<u32>>,
) {
    let clicks = Rc::new(Cell::new(0_u32));
    let doubles = Rc::new(Cell::new(0_u32));
    let (clicked, doubled) = (Rc::clone(&clicks), Rc::clone(&doubles));
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let clicked = Rc::clone(&clicked);
        let doubled = Rc::clone(&doubled);
        Box::new(
            zgui_elements::column()
                .class("root")
                .on(zgui_view::events::CLICK, move |_| {
                    clicked.set(clicked.get() + 1);
                })
                .on(zgui_view::events::DOUBLE_CLICK, move |_| {
                    doubled.set(doubled.get() + 1);
                })
                .into_view()
                .build(cx),
        )
    });
    app.settle(4);
    (app, clicks, doubles)
}

/// A press and a release at `x`, at `at` after the start.
fn click(app: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, x: f32, at: u64) {
    let at = Duration::from_millis(at);
    app.deliver_to_first(pointer(PointerAction::Moved, x, at));
    app.deliver_to_first(pointer(PointerAction::Pressed, x, at));
    app.deliver_to_first(pointer(PointerAction::Released, x, at));
    app.settle(2);
}

#[test]
fn a_second_click_soon_after_the_first_is_a_double_click() {
    let (mut app, clicks, doubles) = counting();
    click(&mut app, 200.0, 0);
    click(&mut app, 201.0, 200);
    assert_eq!(clicks.get(), 2, "both presses still click");
    assert_eq!(doubles.get(), 1);
    click(&mut app, 201.0, 300);
    assert_eq!(doubles.get(), 1, "a third press starts a new count");
}

#[test]
fn a_late_or_distant_second_click_is_no_double_click() {
    let (mut app, clicks, doubles) = counting();
    click(&mut app, 200.0, 0);
    click(&mut app, 200.0, 900);
    click(&mut app, 260.0, 1000);
    assert_eq!(clicks.get(), 3);
    assert_eq!(doubles.get(), 0);
}
