//! A second quick click on the same element is a double click.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{
    KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerButton,
    PointerEvent, Timestamp,
};

/// The sheet the fixture is styled by: one control that covers the window.
const CSS: &str = "control { display: block; width: 400px; height: 300px }";

/// A primary button event at `x`, `millis` after the start.
fn pointer(action: PointerAction, x: f32, millis: u64) -> SurfaceEvent {
    let mut event = PointerEvent::mouse(Point::new(CssPx(x), CssPx(150.0)));
    if !matches!(action, PointerAction::Moved) {
        event = event.with_button(PointerButton::Primary);
    }
    SurfaceEvent::Pointer {
        action,
        event,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::from_origin(Duration::from_millis(millis)),
    }
}

/// A window with one focusable control that records its clicks and double clicks, in order.
fn recording() -> (
    zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    Rc<RefCell<Vec<&'static str>>>,
) {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&heard);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let clicked = Rc::clone(&log);
        let doubled = Rc::clone(&log);
        Box::new(
            zgui_elements::control()
                .tabindex(zgui_elements::Focus::Sequential)
                .on(zgui_view::events::CLICK, move |_| {
                    clicked.borrow_mut().push("click");
                })
                .on(zgui_view::events::DOUBLE_CLICK, move |_| {
                    doubled.borrow_mut().push("double_click");
                })
                .into_view()
                .build(cx),
        )
    });
    app.settle(4);
    app.deliver_to_first(pointer(PointerAction::Moved, 200.0, 0));
    app.settle(4);
    (app, heard)
}

/// A press and a release at `x`, both at `millis`.
fn click(app: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, x: f32, millis: u64) {
    app.deliver_to_first(pointer(PointerAction::Pressed, x, millis));
    app.deliver_to_first(pointer(PointerAction::Released, x, millis));
    app.settle(4);
}

#[test]
fn a_second_quick_click_dispatches_a_double_click_after_its_click() {
    let (mut app, heard) = recording();
    click(&mut app, 200.0, 10);
    click(&mut app, 201.0, 200);
    assert_eq!(*heard.borrow(), ["click", "click", "double_click"]);
}

#[test]
fn a_third_click_starts_a_new_pair() {
    let (mut app, heard) = recording();
    click(&mut app, 200.0, 10);
    click(&mut app, 200.0, 100);
    click(&mut app, 200.0, 200);
    assert_eq!(*heard.borrow(), ["click", "click", "double_click", "click"]);
}

#[test]
fn a_slow_or_distant_second_click_is_no_double_click() {
    let (mut app, heard) = recording();
    click(&mut app, 200.0, 10);
    click(&mut app, 200.0, 1_000);
    click(&mut app, 260.0, 1_100);
    assert_eq!(*heard.borrow(), ["click", "click", "click"]);
}

#[test]
fn a_key_activation_between_two_clicks_ends_the_pair() {
    let (mut app, heard) = recording();
    click(&mut app, 200.0, 10);
    app.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::from_origin(Duration::from_millis(50)),
    });
    app.settle(4);
    click(&mut app, 200.0, 100);
    assert_eq!(*heard.borrow(), ["click", "click", "click"]);
}
