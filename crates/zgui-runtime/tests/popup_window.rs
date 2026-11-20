//! Pop-up windows placed against a shell window.
//!
//! A pop-up opens against the surface of its parent window, carries the grab serial the parent
//! read from a press, and closes with a reason the application can read: when the desktop
//! dismisses it, and when its parent closes first.

mod support;

use zgui_geom::{CssPx, Edges, Point, Rect, Size};
use zgui_platform::{CloseReason, InputSerial, Surface, SurfaceEvent, SurfaceRole};
use zgui_platform_headless::{Harness, Headless};
use zgui_reactive::prelude::*;
use zgui_runtime::windows::{LayerOptions, PopupOptions, WindowHandle, WindowOptions};
use zgui_runtime::{App, Runtime};
use zgui_view::{BuildCx, IntoView, View};

/// An application with one bar across the top of the screen.
fn bar() -> (Harness<Runtime>, WindowHandle) {
    let handler = App::new()
        .with_layer(
            LayerOptions::top()
                .with_anchors(Edges::new(true, true, false, true))
                .with_size(0.0, 34.0),
        )
        .with_renderer(Box::new(support::capture))
        .into_handler(|cx: &mut BuildCx<'_>| {
            Box::new(zgui_elements::column().into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::over(Headless::new(), handler);
    harness.settle(8);
    let bar = harness.app().windows()[0].handle().clone();
    (harness, bar)
}

/// A rectangle on the bar a menu hangs from.
fn item() -> Rect<CssPx, zgui_geom::Css> {
    Rect::new(
        Point::new(CssPx(790.0), CssPx(0.0)),
        Size::new(CssPx(20.0), CssPx(34.0)),
    )
}

/// Opens a menu against `parent` with the grab serial `parent` reports.
fn open_menu(harness: &Harness<Runtime>, parent: &WindowHandle) -> WindowHandle {
    let serial = parent.input_serial();
    harness.app().scope().with(|| {
        zgui_runtime::use_windows().open(
            WindowOptions::popup(
                parent,
                PopupOptions::new(item())
                    .with_grab(serial)
                    .with_size(200.0, 150.0),
            ),
            zgui_elements::column,
        )
    })
}

#[test]
fn a_menu_opens_against_its_shell_window_with_the_press_it_was_opened_by() {
    let (mut harness, bar) = bar();
    let bar_surface = harness.platform().offscreens()[0].clone();
    bar_surface.set_input_serial(Some(InputSerial::new(42)));

    let menu = open_menu(&harness, &bar);
    harness.settle(8);
    assert!(menu.is_open());
    assert!(
        !menu.focused().get_untracked(),
        "the desktop gives the focus"
    );

    let surface = harness.platform().offscreens()[1].clone();
    let SurfaceRole::Popup(placement) = surface.requested_attributes().role else {
        panic!("the menu is a pop-up");
    };
    assert_eq!(placement.parent, bar_surface.id());
    assert_eq!(placement.anchor_rect, item());
    assert_eq!(placement.grab, Some(InputSerial::new(42)));
    harness.shut_down();
}

#[test]
fn a_menu_the_desktop_dismisses_closes_and_says_why() {
    let (mut harness, bar) = bar();
    let menu = open_menu(&harness, &bar);
    harness.settle(8);
    let surface = harness.platform().offscreens()[1].id();

    harness.deliver(surface, SurfaceEvent::Closed(CloseReason::Dismissed));
    harness.settle(8);
    assert!(!menu.is_open());
    assert_eq!(
        menu.close_reason().get_untracked(),
        Some(CloseReason::Dismissed)
    );
    assert!(bar.is_open(), "the parent stays");
    assert_eq!(harness.platform().offscreens().len(), 1);
    harness.shut_down();
}

#[test]
fn a_menu_closes_before_its_parent() {
    let (mut harness, bar) = bar();
    let menu = open_menu(&harness, &bar);
    harness.settle(8);

    bar.close();
    harness.settle(8);
    assert_eq!(
        menu.close_reason().get_untracked(),
        Some(CloseReason::Dismissed)
    );
    assert_eq!(
        bar.close_reason().get_untracked(),
        Some(CloseReason::Application)
    );
    assert!(harness.platform().offscreens().is_empty());
}

#[test]
fn a_menu_whose_parent_is_not_open_does_not_open() {
    let (mut harness, bar) = bar();
    bar.close();
    harness.settle(8);
    let menu = open_menu(&harness, &bar);
    harness.settle(8);
    assert!(!menu.is_open());
    assert!(harness.platform().offscreens().is_empty());
}
