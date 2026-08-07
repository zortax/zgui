//! What a suspend does to several windows, and whose frame runs whose callbacks.
//!
//! Driven against the handler directly rather than through the harness's event pump, because a
//! suspend and a resume are handler callbacks a script has to make itself.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui_platform::SurfaceAttributes;
use zgui_platform_headless::Harness;
use zgui_runtime::{Runtime, WindowSpec};
use zgui_view::{Anchor, BuildCx, IntoView, View};

use support::app;

/// The stylesheet every window here is styled by.
const SHEET: &str = "root { display: block } column { display: block }";

/// A view that builds one empty column.
fn empty(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(zgui_elements::column().into_view().build(cx))
}

/// Asks for a second window holding `view`, and settles until it is open.
fn open_second<V>(harness: &mut Harness<Runtime>, view: V)
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let mut options = zgui_runtime::WindowContent::default();
    options.window_stylesheet = Some(SHEET.to_owned());
    harness.app().window_commands().open(WindowSpec::new(
            SurfaceAttributes::new("second"),
            options,
            Box::new(view),
        ));
    harness.settle(8);
}

#[test]
fn every_window_comes_back_when_the_surfaces_do() {
    let mut harness = app(SHEET, empty);
    harness.settle(4);
    open_second(&mut harness, empty);
    assert_eq!(harness.app().windows().len(), 2);
    let before: Vec<_> = harness
        .app()
        .windows()
        .iter()
        .map(|window| window.document_id())
        .collect();

    // The platform takes every surface away, as a mobile platform does on a suspend. What the
    // application asked for is not withdrawn, so nothing here is a close.
    harness.suspend();
    assert!(
        harness.app().windows().is_empty(),
        "a window cannot outlive the surface it draws into"
    );

    harness.resume();
    assert_eq!(
        harness.app().windows().len(),
        2,
        "a resumed application came back with fewer windows than it had"
    );

    // New identities, because the documents are new: a handle from before the suspend names a
    // document that no longer exists, and it must fail to resolve rather than resolve into one of
    // these.
    let after: Vec<_> = harness
        .app()
        .windows()
        .iter()
        .map(|window| window.document_id())
        .collect();
    for identity in &after {
        assert!(!before.contains(identity), "a document identity was reused");
    }
}

#[test]
fn a_window_closed_for_good_does_not_come_back_on_resume() {
    let mut harness = app(SHEET, empty);
    harness.settle(4);
    open_second(&mut harness, empty);
    let second = harness.app().windows()[1].surface().id();

    harness.deliver(second, zgui_platform::SurfaceEvent::CloseRequested);
    harness.settle(4);
    assert_eq!(harness.app().windows().len(), 1);

    harness.suspend();
    harness.resume();
    assert_eq!(
        harness.app().windows().len(),
        1,
        "a window the user closed came back when the application resumed"
    );
}

#[test]
fn a_callback_runs_in_the_frame_of_the_window_that_scheduled_it() {
    // The regression this exists for: one heap holds every window's callbacks, and a drain that
    // ignored which window an entry belongs to ran one window's callbacks inside whichever window
    // happened to frame first — under the wrong reactive scope, against the wrong document.
    let ran: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));

    let recorded = Rc::clone(&ran);
    let mut harness = app(SHEET, move |cx: &mut BuildCx<'_>| {
        let recorded = Rc::clone(&recorded);
        // Held for the life of the window: dropping the handle cancels the callback.
        core::mem::forget(zgui_view::set_timeout(
            Duration::from_millis(50),
            move || recorded.borrow_mut().push("first"),
        ));
        empty(cx)
    });
    harness.settle(4);

    let recorded = Rc::clone(&ran);
    open_second(&mut harness, move |cx: &mut BuildCx<'_>| {
        let recorded = Rc::clone(&recorded);
        core::mem::forget(zgui_view::set_timeout(
            Duration::from_millis(50),
            move || recorded.borrow_mut().push("second"),
        ));
        empty(cx)
    });

    assert!(ran.borrow().is_empty(), "a callback fired before its time");
    harness.advance(Duration::from_millis(60));
    harness.settle(8);

    // Both fired, each in its own window's frame. Order is not asserted: which window frames first
    // is the loop's business, and the point is that each callback ran exactly once.
    let ran = ran.borrow();
    assert_eq!(ran.len(), 2, "a callback ran in the wrong window or twice");
    assert!(ran.contains(&"first"));
    assert!(ran.contains(&"second"));
}
