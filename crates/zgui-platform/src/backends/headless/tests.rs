//! The headless backend driven through the contract.

use std::sync::Arc;
use std::time::Duration;

use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use zgui_geom::{CssPx, Point};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

use crate::app::{AppHandler, IdlePolicy, WakeReason};
use crate::backends::headless::Headless;
use crate::backends::headless::app::RecordingApp;
use crate::clipboard::{ClipboardData, ClipboardFormat, ClipboardKind, ClipboardWriteOptions};
use crate::cx::PlatformCx;
use crate::surface::{SurfaceAttributes, SurfaceEvent, SurfaceId};

fn a11y_update() -> TreeUpdate {
    let root = NodeId(1);
    TreeUpdate {
        nodes: vec![(root, Node::new(Role::Window))],
        tree: Some(Tree::new(root)),
        tree_id: TreeId::ROOT,
        focus: root,
    }
}

#[test]
fn a_headless_backend_satisfies_the_whole_contract() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    let mut app = RecordingApp::default();

    app.surfaces_available(cx);
    assert_eq!(app.surfaces_available, 1);
    assert_eq!(cx.surfaces().len(), 1);

    let surface = cx.surfaces().remove(0);
    assert_eq!(cx.surface(surface.id()).map(|s| s.id()), Some(surface.id()));
    assert!(cx.surface(SurfaceId::new(999)).is_none());

    app.surface_event(cx, surface.id(), SurfaceEvent::RedrawRequested);
    assert_eq!(app.events.len(), 1);

    app.wake(cx, WakeReason::DeviceLost);
    assert_eq!(app.wakes, 1);

    assert!(!cx.is_exiting());
    cx.request_exit();
    assert!(cx.is_exiting());
}

#[test]
fn the_headless_backend_can_carry_scripted_input() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    let mut app = RecordingApp::default();
    app.surfaces_available(cx);
    let surface = cx.surfaces().remove(0);

    for action in [
        PointerAction::Entered,
        PointerAction::Pressed,
        PointerAction::Released,
    ] {
        app.surface_event(
            cx,
            surface.id(),
            SurfaceEvent::Pointer {
                action,
                event: PointerEvent::mouse(Point::new(CssPx(10.0), CssPx(10.0))),
                modifiers: Modifiers::NONE,
                timestamp: cx.clock().timestamp(),
            },
        );
    }
    assert_eq!(app.events.len(), 3);
}

#[test]
fn a_virtual_clock_crossing_a_deadline_is_what_asks_for_the_frame() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    let mut app = RecordingApp::default();
    app.surfaces_available(cx);
    let recorded = platform
        .offscreen(SurfaceId::new(1))
        .expect("the surface was just created");
    assert_eq!(recorded.redraws(), 0);

    let deadline = cx.clock().now() + Duration::from_millis(700);
    app.park_until = Some(deadline);
    assert_eq!(app.idle(cx), IdlePolicy::BlockUntil(deadline));

    // Not yet: the deadline has not been reached, so nothing is asked for.
    assert!(!platform.advance(Duration::from_millis(699), app.park_until));
    assert_eq!(app.deadlines_reached, 0);
    assert_eq!(recorded.redraws(), 0);

    // The last millisecond crosses it, and the crossing itself produces the redraw request.
    assert!(platform.advance(Duration::from_millis(1), app.park_until));
    app.deadline_reached(cx);
    assert_eq!(app.deadlines_reached, 1);
    assert_eq!(
        recorded.redraws(),
        1,
        "the deadline was reported reached without a frame being asked for"
    );
    assert_eq!(app.idle(cx), IdlePolicy::Block);
    assert_eq!(
        cx.clock().timestamp().since_origin(),
        Duration::from_millis(700)
    );
}

#[test]
fn the_headless_clipboard_round_trips_through_the_contract() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    cx.clipboard()
        .write(
            ClipboardKind::Primary,
            ClipboardData::from("selected"),
            ClipboardWriteOptions::default(),
        )
        .expect("memory is writable");
    assert_eq!(
        cx.clipboard()
            .read_blocking(ClipboardKind::Primary, ClipboardFormat::Text)
            .expect("memory is readable")
            .as_text(),
        Some("selected")
    );
    // The two clipboards are separate, which is the property a shared slot would break.
    assert!(
        cx.clipboard()
            .read_blocking(ClipboardKind::Standard, ClipboardFormat::Text)
            .is_err()
    );
}

#[test]
fn an_asynchronous_clipboard_read_is_answered_by_a_wake_naming_the_same_request() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    let serial = cx
        .clipboard()
        .read(ClipboardKind::Standard, ClipboardFormat::Text);

    // A backend answers by waking the loop; nothing else closes the request.
    cx.waker().wake(WakeReason::ClipboardRead {
        serial,
        result: Ok(ClipboardData::from("pasted")),
    });

    let delivered = platform.drain_wakes();
    assert_eq!(delivered.len(), 1);
    match &delivered[0] {
        WakeReason::ClipboardRead {
            serial: answered,
            result,
        } => {
            assert_eq!(*answered, serial);
            assert_eq!(
                result.as_ref().expect("the read succeeded").as_text(),
                Some("pasted")
            );
        }
        other => panic!("the answer arrived as {other:?}"),
    }
    assert!(platform.drain_wakes().is_empty());
}

#[test]
fn a_headless_surface_publishes_accessibility_updates() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    let surface = cx
        .create_surface(&SurfaceAttributes::new("headless"))
        .expect("a headless surface is always creatable");
    let recorded = platform
        .offscreen(surface.id())
        .expect("the surface was just created");
    let mut built = 0;
    surface.push_a11y_update(&mut || {
        built += 1;
        a11y_update()
    });
    assert_eq!(built, 1);
    assert_eq!(recorded.a11y_updates(), 1);
}

#[test]
fn a_surface_is_created_hidden_and_is_shown_only_when_it_is_asked_to_be() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    let surface = cx
        .create_surface(&SurfaceAttributes::new("headless"))
        .expect("a headless surface is always creatable");
    let recorded = platform
        .offscreen(surface.id())
        .expect("the surface was just created");
    assert!(!recorded.is_visible());
    surface.set_visible(true);
    assert!(recorded.is_visible());
}

#[test]
fn a_headless_surface_offers_no_graphics_handles() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    let surface = cx
        .create_surface(&SurfaceAttributes::new("headless"))
        .expect("a headless surface is always creatable");
    assert!(surface.gpu().is_none());
}

#[test]
fn the_context_and_the_handler_are_usable_behind_pointers() {
    let platform = Headless::new();
    let cx: Box<dyn PlatformCx> = Box::new(platform);
    let mut app: Box<dyn AppHandler> = Box::new(RecordingApp::default());
    app.surfaces_available(cx.as_ref());
    assert_eq!(cx.surfaces().len(), 1);

    // A waker outlives the callback that produced it, and crosses threads.
    let waker = cx.waker();
    std::thread::spawn(move || waker.wake(WakeReason::DeviceLost))
        .join()
        .expect("the thread finished");
}

#[test]
fn a_surface_is_shared_and_thread_safe_because_a_redraw_can_come_from_anywhere() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    let surface = cx
        .create_surface(&SurfaceAttributes::new("headless"))
        .expect("a headless surface is always creatable");
    let recorded = platform
        .offscreen(surface.id())
        .expect("the surface was just created");
    let sent = Arc::clone(&surface);
    std::thread::spawn(move || sent.request_redraw())
        .join()
        .expect("the thread finished");
    assert_eq!(surface.id(), SurfaceId::new(1));
    assert_eq!(recorded.redraws(), 1);
}

#[test]
fn the_headless_backend_reports_a_timestamp_that_only_a_test_can_move() {
    let platform = Headless::new();
    let cx: &dyn PlatformCx = &platform;
    assert_eq!(cx.clock().timestamp(), Timestamp::ORIGIN);
    platform.advance(Duration::from_secs(5), None);
    assert_eq!(
        cx.clock().timestamp().since_origin(),
        Duration::from_secs(5)
    );
}
