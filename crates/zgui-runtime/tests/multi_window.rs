//! Two windows in one process: their identities, their events, their state and their lives.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zgui_platform::{PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId};
use zgui_platform_headless::Harness;
use zgui_runtime::{
    CloseCallbacks, CloseResponse, ExitPolicy, Runtime, WindowSpec, WindowStatus, WindowToken,
};
use zgui_reactive::prelude::{Get, Set};
use zgui_view::{Anchor, BuildCx, IntoView, View};

use support::app;

/// The stylesheet every window here is styled by.
const SHEET: &str = "root { display: block } column { display: block }";

/// A view that builds one empty column, for a window whose contents are not the point.
fn empty(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(zgui_elements::column().into_view().build(cx))
}

/// Asks for a second window holding `view`, and settles until it is open.
fn open_second<V>(harness: &mut Harness<Runtime>, view: V) -> WindowToken
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let mut options = zgui_runtime::WindowContent::default();
    options.window_stylesheet = Some(SHEET.to_owned());
    let token = harness.app().window_commands().open(WindowSpec::new(
            SurfaceAttributes::new("second"),
            options,
            Box::new(view),
        ));
    harness.settle(8);
    token
}

/// The surfaces the application has open, in the order they were created.
fn surfaces(harness: &Harness<Runtime>) -> Vec<SurfaceId> {
    harness
        .app()
        .windows()
        .iter()
        .map(|window| window.surface().id())
        .collect()
}

#[test]
fn a_second_window_opens_and_is_a_second_document() {
    let mut harness = app(SHEET, empty);
    harness.settle(4);
    assert_eq!(harness.app().windows().len(), 1);

    let token = open_second(&mut harness, empty);

    assert_eq!(harness.app().windows().len(), 2, "the second window is open");
    assert!(matches!(
        harness.app().window_commands().status(token),
        WindowStatus::Open(_)
    ));
    // Two windows are two documents. A node handle carries the document it was minted in, and
    // handing both windows the same identity is what would let one window's handle resolve inside
    // the other's tree while passing every ownership assertion on the way.
    let first = harness.app().windows()[0].document_id();
    let second = harness.app().windows()[1].document_id();
    assert_ne!(first, second);
}

#[test]
fn every_window_belongs_to_the_same_application() {
    // A window opened while the application runs is the same application's. Without this the
    // desktop sees an unnamed window: it carries no icon, no window rule selects it, and it groups
    // under nothing rather than beside the window it was opened from.
    let handler = zgui_runtime::App::new()
        .with_title("test")
        .with_application_id("dev.zgui.Test")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(support::capture))
        .into_handler(empty)
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(4);
    open_second(&mut harness, empty);

    let named: Vec<_> = harness
        .platform()
        .offscreens()
        .iter()
        .map(|surface| surface.requested_attributes().application_id.clone())
        .collect();
    assert_eq!(named.len(), 2);
    for id in named {
        assert_eq!(
            id.as_ref().map(zgui_vocab::SharedString::as_str),
            Some("dev.zgui.Test"),
            "a window opened while the application ran belongs to no application"
        );
    }
}

#[test]
fn an_event_reaches_only_the_window_it_names() {
    let mut harness = app(SHEET, empty);
    harness.settle(4);
    open_second(&mut harness, empty);

    let ids = surfaces(&harness);
    let before: Vec<_> = harness
        .app()
        .windows()
        .iter()
        .map(|window| window.surface().size())
        .collect();

    harness.deliver(
        ids[1],
        SurfaceEvent::Resized(zgui_geom::Size::new(
            zgui_geom::DevicePx(812.0),
            zgui_geom::DevicePx(377.0),
        )),
    );
    harness.settle(4);

    let after: Vec<_> = harness
        .app()
        .windows()
        .iter()
        .map(|window| window.surface().size())
        .collect();
    assert_eq!(after[0], before[0], "the other window was resized too");
    assert_ne!(after[1], before[1], "the named window was not resized");
}

#[test]
fn what_the_application_provides_is_shared_and_what_a_window_provides_is_its_own() {
    /// A value one window provides for itself.
    #[derive(Clone)]
    struct Local(&'static str);
    /// A value the application provides above every window.
    #[derive(Clone)]
    struct Shared(&'static str);

    let seen: Rc<RefCell<Vec<(Option<String>, Option<String>)>>> =
        Rc::new(RefCell::new(Vec::new()));

    let recorded = Rc::clone(&seen);
    let mut harness = app(SHEET, move |cx: &mut BuildCx<'_>| {
        zgui_reactive::provide_local_context(Local("first"));
        recorded.borrow_mut().push((
            zgui_reactive::use_local_context::<Local>().map(|value| value.0.to_owned()),
            zgui_reactive::use_local_context::<Shared>().map(|value| value.0.to_owned()),
        ));
        empty(cx)
    });
    // Above every window, in the scope each window's own is mounted under.
    harness
        .app()
        .scope()
        .with(|| zgui_reactive::provide_local_context(Shared("everyone")));
    harness.settle(4);

    let recorded = Rc::clone(&seen);
    open_second(&mut harness, move |cx: &mut BuildCx<'_>| {
        recorded.borrow_mut().push((
            zgui_reactive::use_local_context::<Local>().map(|value| value.0.to_owned()),
            zgui_reactive::use_local_context::<Shared>().map(|value| value.0.to_owned()),
        ));
        empty(cx)
    });

    let seen = seen.borrow();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].0.as_deref(), Some("first"));
    assert_eq!(
        seen[1].0, None,
        "one window saw the context another window provided"
    );
    // The application's own scope is above both, so both resolve through it.
    assert_eq!(seen[1].1.as_deref(), Some("everyone"));
}

#[test]
fn a_write_from_one_window_is_drawn_by_the_window_that_reads_it() {
    // The regression this exists for: the reactive flush is thread-wide, so the frame that runs an
    // effect can be a frame in a *different* window from the one the effect writes. That flush
    // services the wake, and without the sweep nothing would ever ask the written window to draw.
    let shared = zgui_reactive::RwSignal::new(0_i32);
    let mut harness = app(SHEET, empty);
    harness.settle(4);

    let drawn = Rc::new(Cell::new(0_i32));
    let recorded = Rc::clone(&drawn);
    open_second(&mut harness, move |cx: &mut BuildCx<'_>| {
        let recorded = Rc::clone(&recorded);
        // An effect in the second window reading a signal the first window writes.
        // Held for the life of the window: a dropped effect stops.
        core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
            recorded.set(shared.get());
        }));
        empty(cx)
    });
    assert_eq!(drawn.get(), 0);

    // Written from outside any window's frame, exactly as a listener in the first window would.
    shared.set(7);
    harness.settle(8);

    assert_eq!(
        drawn.get(),
        7,
        "the window that reads the signal never ran the frame that shows it"
    );
}

#[test]
fn closing_one_window_leaves_the_other_running() {
    let mut harness = app(SHEET, empty);
    harness.settle(4);
    open_second(&mut harness, empty);
    let ids = surfaces(&harness);

    harness.deliver(ids[1], SurfaceEvent::CloseRequested);
    harness.settle(4);
    assert_eq!(harness.app().windows().len(), 1);
    assert!(
        !harness.platform().is_exiting(),
        "one window closing stopped an application that still has one"
    );

    harness.deliver(ids[0], SurfaceEvent::CloseRequested);
    harness.settle(4);
    assert!(harness.app().windows().is_empty());
    assert!(
        harness.platform().is_exiting(),
        "the last window closed and the application kept running"
    );
}

#[test]
fn a_closed_window_is_destroyed_and_not_merely_forgotten() {
    // Dropping the application's own half of a window is not closing it. Whatever the backend holds
    // is what keeps the window on the screen, so a close that only forgot it would leave a window
    // that no longer draws, no longer responds, and cannot be closed by any means at all.
    let mut harness = app(SHEET, empty);
    harness.settle(4);
    open_second(&mut harness, empty);
    assert_eq!(harness.platform().offscreens().len(), 2);
    let ids = surfaces(&harness);

    harness.deliver(ids[1], SurfaceEvent::CloseRequested);
    harness.settle(4);

    let left = harness.platform().offscreens();
    assert_eq!(left.len(), 1, "the surface outlived the window that closed");
    assert_eq!(
        zgui_platform::Surface::id(left[0].as_ref()),
        ids[0],
        "the wrong surface was destroyed"
    );
}

#[test]
fn a_window_the_application_closes_is_destroyed_too() {
    let mut harness = app(SHEET, empty);
    harness.settle(4);
    let token = open_second(&mut harness, empty);
    assert_eq!(harness.platform().offscreens().len(), 2);

    harness.app().window_commands().close(token);
    harness.settle(4);
    assert_eq!(
        harness.platform().offscreens().len(),
        1,
        "the surface outlived the window the application closed"
    );
}

#[test]
fn a_close_the_user_asked_for_can_be_refused() {
    let refusals = Rc::new(Cell::new(2_u32));
    let close = Rc::new(RefCell::new(CloseCallbacks::default()));
    let counted = Rc::clone(&refusals);
    close.borrow_mut().insert(Box::new(move || {
        // Refuses twice and then relents, as a document with something to save would.
        if counted.get() == 0 {
            return CloseResponse::Close;
        }
        counted.set(counted.get() - 1);
        CloseResponse::Veto
    }));

    let mut harness = app(SHEET, empty);
    harness.settle(4);
    let mut options = zgui_runtime::WindowContent::default();
    options.window_stylesheet = Some(SHEET.to_owned());
    harness.app().window_commands().open(WindowSpec::new(
            SurfaceAttributes::new("second"),
            options,
            Box::new(empty),
        )
        .with_close_callbacks(Rc::clone(&close)));
    harness.settle(8);
    let ids = surfaces(&harness);

    for attempt in 0..2 {
        harness.deliver(ids[1], SurfaceEvent::CloseRequested);
        harness.settle(2);
        assert_eq!(
            harness.app().windows().len(),
            2,
            "attempt {attempt} closed a window that refused"
        );
    }

    harness.deliver(ids[1], SurfaceEvent::CloseRequested);
    harness.settle(4);
    assert_eq!(harness.app().windows().len(), 1, "the window never relented");
}

#[test]
fn the_platform_taking_a_window_is_not_a_close_to_refuse() {
    let close = Rc::new(RefCell::new(CloseCallbacks::default()));
    close.borrow_mut().insert(Box::new(|| CloseResponse::Veto));

    let mut harness = app(SHEET, empty);
    harness.settle(4);
    let mut options = zgui_runtime::WindowContent::default();
    options.window_stylesheet = Some(SHEET.to_owned());
    harness.app().window_commands().open(
        WindowSpec::new(SurfaceAttributes::new("second"), options, Box::new(empty))
            .with_close_callbacks(close),
    );
    harness.settle(8);
    let ids = surfaces(&harness);

    // Destroyed, not CloseRequested: the window is already gone and there is nothing to refuse.
    harness.deliver(ids[1], SurfaceEvent::Destroyed);
    harness.settle(4);
    assert_eq!(harness.app().windows().len(), 1);
}

#[test]
fn an_application_can_close_a_window_that_refuses_the_user() {
    let close = Rc::new(RefCell::new(CloseCallbacks::default()));
    close.borrow_mut().insert(Box::new(|| CloseResponse::Veto));

    let mut harness = app(SHEET, empty);
    harness.settle(4);
    let mut options = zgui_runtime::WindowContent::default();
    options.window_stylesheet = Some(SHEET.to_owned());
    let token = harness.app().window_commands().open(
        WindowSpec::new(SurfaceAttributes::new("second"), options, Box::new(empty))
            .with_close_callbacks(close),
    );
    harness.settle(8);
    assert_eq!(harness.app().windows().len(), 2);

    // The callbacks answer the *user* asking. An application closing its own window has decided.
    harness.app().window_commands().close(token);
    harness.settle(4);
    assert_eq!(harness.app().windows().len(), 1);
    assert!(matches!(
        harness.app().window_commands().status(token),
        WindowStatus::Closed
    ));
}

#[test]
fn an_application_that_stops_only_when_it_says_so_outlives_every_window() {
    let handler = zgui_runtime::App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(support::capture))
        .with_exit_policy(ExitPolicy::Explicit)
        .into_handler(empty)
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(4);
    let ids = surfaces(&harness);

    harness.deliver(ids[0], SurfaceEvent::CloseRequested);
    harness.settle(4);
    assert!(harness.app().windows().is_empty());
    assert!(
        !harness.platform().is_exiting(),
        "an application that stops only on request stopped on its own"
    );

    harness.app().window_commands().quit();
    harness.settle(4);
    assert!(harness.platform().is_exiting());
}

#[test]
fn an_application_tied_to_its_first_window_stops_with_it() {
    let handler = zgui_runtime::App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(support::capture))
        .with_exit_policy(ExitPolicy::WhenPrimaryCloses)
        .into_handler(empty)
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(4);
    open_second(&mut harness, empty);
    let ids = surfaces(&harness);

    // The window it launched with, closed while another is still open.
    harness.deliver(ids[0], SurfaceEvent::CloseRequested);
    harness.settle(4);
    assert!(
        harness.platform().is_exiting(),
        "the window the application launched with closed and it kept running"
    );
}
